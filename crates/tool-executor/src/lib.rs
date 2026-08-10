mod process;
mod write;
mod write_diff;

pub use process::{
    CLEANUP_UNPROVEN_PUBLIC_ERROR, ClaimedObserver, GovernedProcessError, GovernedProcessExecution,
    GovernedProcessOutcome, GovernedProcessToolGateway, LiveProcessOutput, PROCESS_CATALOG,
    PROCESS_OUTPUT_BYTE_CAP, PROCESS_RUN_ARGV_BYTE_CAP, PROCESS_RUN_ARGV_COUNT_CAP,
    PROCESS_TIMEOUT, PreparedExecProposal, PreparedShellExecProposal, ProcessOutputChunk,
    ProcessOutputStream, SHELL_PROGRAM_LABEL, SealedProcessSpec, SealedShellSpec,
    build_clean_environment, validate_process_run_arguments, validate_program_catalog,
};
pub use write::{
    BaseSnapshot, GovernedWriteApplyOutcome, GovernedWriteError, GovernedWriteToolGateway,
    PreparedWriteProposal, SealedWriteContents, content_sha256_digest,
};
pub use write_diff::{BoundedDiff, DIFF_BYTE_CAP, DIFF_LINE_CAP, build_bounded_unified_diff};

use std::fs::{self, File, Metadata};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use keeper_agent_protocol::{
    FsListEntry, FsListOutput, FsReadOutput, FsSearchMatch, FsSearchOutput, ToolArguments,
    ToolContentKind, ToolEntryKind, ToolExecutionState, ToolPublicError, ToolPublicErrorCode,
    ToolResult, ValidatedToolProposal, ValidatedToolResult,
};
use keeper_storage_sqlite::{
    SqliteToolCallRepository, ToolCallReservationOutcome, ToolCallStoreError,
    ToolExecutionClaimOutcome,
};
use serde_json::Value;
use thiserror::Error;

const MAX_SEARCH_LINE_BYTES: usize = 65_536;
const MAX_SEARCH_PREVIEW_BYTES: usize = 2_048;
const MAX_SEARCH_FILES: usize = 100_000;

#[derive(Debug, Error)]
pub enum ToolExecutorError {
    #[error("project capability root could not be established")]
    InvalidProjectRoot,
    #[error("executor produced an invalid protocol result")]
    InvalidResult,
}

#[derive(Debug, Clone)]
pub struct ProjectCapability {
    project_id: String,
    canonical_root: PathBuf,
}

impl ProjectCapability {
    pub fn new(
        project_id: impl Into<String>,
        root: impl AsRef<Path>,
    ) -> Result<Self, ToolExecutorError> {
        let canonical_root = root
            .as_ref()
            .canonicalize()
            .map_err(|_| ToolExecutorError::InvalidProjectRoot)?;
        if !canonical_root.is_dir() {
            return Err(ToolExecutorError::InvalidProjectRoot);
        }
        Ok(Self {
            project_id: project_id.into(),
            canonical_root,
        })
    }

    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    pub fn canonical_root(&self) -> &Path {
        &self.canonical_root
    }
}

pub trait CancellationSignal: Send + Sync {
    fn is_cancelled(&self) -> bool;
}

#[derive(Debug, Default)]
pub struct NeverCancelled;

impl CancellationSignal for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[derive(Debug, Default)]
pub struct ReadToolExecutor;

impl ReadToolExecutor {
    pub fn execute(
        &self,
        proposal: &ValidatedToolProposal,
        capability: &ProjectCapability,
        cancellation: &dyn CancellationSignal,
    ) -> Result<ValidatedToolResult, ToolExecutorError> {
        if proposal.project_id() != capability.project_id {
            return terminal_result(
                proposal,
                ToolExecutionState::Failed,
                None,
                Some(ToolPublicErrorCode::ProjectMismatch),
            );
        }

        let started = Instant::now();
        let deadline = started
            .checked_add(Duration::from_millis(proposal.manifest().limits.timeout_ms))
            .unwrap_or(started);
        if let Err(failure) = checkpoint(cancellation, deadline) {
            return failure_result(proposal, failure);
        }

        let current_root = match capability.canonical_root.canonicalize() {
            Ok(root) if root == capability.canonical_root && root.is_dir() => root,
            _ => {
                return terminal_result(
                    proposal,
                    ToolExecutionState::Failed,
                    None,
                    Some(ToolPublicErrorCode::PathRejected),
                );
            }
        };

        let operation = match proposal.arguments() {
            ToolArguments::FsList(arguments) => execute_list(
                &current_root,
                &arguments.path,
                arguments.max_results,
                proposal.manifest().limits.output_byte_cap,
                cancellation,
                deadline,
            ),
            ToolArguments::FsRead(arguments) => execute_read(
                &current_root,
                &arguments.path,
                arguments.offset_bytes,
                arguments.max_bytes,
                proposal.manifest().limits.output_byte_cap,
                cancellation,
                deadline,
            ),
            ToolArguments::FsSearch(arguments) => execute_search(
                &current_root,
                &arguments.path,
                &arguments.query,
                arguments.case_sensitive,
                arguments.max_results,
                proposal.manifest().limits.output_byte_cap,
                cancellation,
                deadline,
            ),
            // Read executor never applies writes. Fail closed with no mutation.
            ToolArguments::FsWrite(_) => Err(OperationFailure::Public(
                ToolPublicErrorCode::ApprovalRequired,
            )),
            // Read executor never spawns processes. A governed process gateway
            // owns this capability behind its independent approval boundary.
            ToolArguments::ProcessRun(_) => Err(OperationFailure::Public(
                ToolPublicErrorCode::ApprovalRequired,
            )),
        };

        match operation {
            Ok(output) => {
                terminal_result(proposal, ToolExecutionState::Succeeded, Some(output), None)
            }
            Err(failure) => failure_result(proposal, failure),
        }
    }
}

#[derive(Debug)]
pub enum GovernedToolOutcome {
    Executed(ValidatedToolResult),
    Replayed(ValidatedToolResult),
    InProgress,
}

#[derive(Debug, Error)]
pub enum GovernedToolError {
    #[error("durable tool-call store rejected the operation")]
    Store(#[from] ToolCallStoreError),
    #[error("read-only executor failed to produce a protocol result")]
    Executor(#[from] ToolExecutorError),
    #[error("tool call identity conflicts with a durable proposal")]
    ReservationConflict,
    #[error("durable tool receipt is missing or invalid")]
    InvalidReceipt,
}

pub struct GovernedReadToolGateway {
    store: SqliteToolCallRepository,
    executor: ReadToolExecutor,
}

impl GovernedReadToolGateway {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, GovernedToolError> {
        Ok(Self {
            store: SqliteToolCallRepository::open(path)?,
            executor: ReadToolExecutor,
        })
    }

    pub fn execute(
        &self,
        proposal: &ValidatedToolProposal,
        capability: &ProjectCapability,
        cancellation: &dyn CancellationSignal,
    ) -> Result<GovernedToolOutcome, GovernedToolError> {
        let current = match self.store.reserve(proposal)? {
            ToolCallReservationOutcome::Reserved(row) => row,
            ToolCallReservationOutcome::Conflict(_) => {
                return Err(GovernedToolError::ReservationConflict);
            }
            ToolCallReservationOutcome::Replay(row) if is_terminal(row.state) => {
                return load_receipt(proposal, row.result_json.as_deref())
                    .map(GovernedToolOutcome::Replayed);
            }
            ToolCallReservationOutcome::Replay(row) => row,
        };

        match current.state {
            ToolExecutionState::Proposed => {
                self.store
                    .transition(proposal, ToolExecutionState::Validated, None)?;
            }
            ToolExecutionState::Validated | ToolExecutionState::Executing => {}
            ToolExecutionState::Succeeded
            | ToolExecutionState::Failed
            | ToolExecutionState::Cancelled => {
                return load_receipt(proposal, current.result_json.as_deref())
                    .map(GovernedToolOutcome::Replayed);
            }
        }

        let owner = format!("gateway-{}", uuid::Uuid::new_v4());
        let lease_ms = proposal.manifest().limits.timeout_ms.saturating_add(5_000);
        match self.store.claim_execution(proposal, &owner, lease_ms)? {
            ToolExecutionClaimOutcome::Busy(_) => {
                return Ok(GovernedToolOutcome::InProgress);
            }
            ToolExecutionClaimOutcome::Terminal(row) => {
                return load_receipt(proposal, row.result_json.as_deref())
                    .map(GovernedToolOutcome::Replayed);
            }
            ToolExecutionClaimOutcome::Claimed(_) => {}
        }

        let result = self.executor.execute(proposal, capability, cancellation)?;
        self.store.complete_execution(proposal, &owner, &result)?;
        Ok(GovernedToolOutcome::Executed(result))
    }
}

fn is_terminal(state: ToolExecutionState) -> bool {
    matches!(
        state,
        ToolExecutionState::Succeeded | ToolExecutionState::Failed | ToolExecutionState::Cancelled
    )
}

fn load_receipt(
    proposal: &ValidatedToolProposal,
    receipt: Option<&str>,
) -> Result<ValidatedToolResult, GovernedToolError> {
    let parsed = receipt
        .ok_or(GovernedToolError::InvalidReceipt)
        .and_then(|value| {
            ToolResult::parse_str(value).map_err(|_| GovernedToolError::InvalidReceipt)
        })?;
    parsed
        .validate_for_proposal(proposal.context(), proposal)
        .map_err(|_| GovernedToolError::InvalidReceipt)
}

#[derive(Debug, Clone, Copy)]
enum OperationFailure {
    Cancelled,
    Public(ToolPublicErrorCode),
}

fn checkpoint(
    cancellation: &dyn CancellationSignal,
    deadline: Instant,
) -> Result<(), OperationFailure> {
    if cancellation.is_cancelled() {
        return Err(OperationFailure::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(OperationFailure::Public(
            ToolPublicErrorCode::DeadlineExceeded,
        ));
    }
    Ok(())
}

fn failure_result(
    proposal: &ValidatedToolProposal,
    failure: OperationFailure,
) -> Result<ValidatedToolResult, ToolExecutorError> {
    match failure {
        OperationFailure::Cancelled => {
            terminal_result(proposal, ToolExecutionState::Cancelled, None, None)
        }
        OperationFailure::Public(code) => {
            terminal_result(proposal, ToolExecutionState::Failed, None, Some(code))
        }
    }
}

fn terminal_result(
    proposal: &ValidatedToolProposal,
    state: ToolExecutionState,
    output: Option<Value>,
    error: Option<ToolPublicErrorCode>,
) -> Result<ValidatedToolResult, ToolExecutorError> {
    ToolResult {
        project_id: proposal.project_id().to_owned(),
        conversation_id: proposal.conversation_id().to_owned(),
        run_id: proposal.run_id(),
        call_id: proposal.call_id().to_owned(),
        tool_id: proposal.tool_id().to_owned(),
        tool_version: proposal.tool_version(),
        proposal_fingerprint: proposal.fingerprint().clone(),
        arguments_digest: proposal.normalized_arguments_digest().clone(),
        state,
        output,
        error: error.map(ToolPublicError::canonical),
    }
    .validate_for_proposal(proposal.context(), proposal)
    .map_err(|_| ToolExecutorError::InvalidResult)
}

fn execute_list(
    root: &Path,
    relative: &str,
    max_results: u32,
    output_byte_cap: u64,
    cancellation: &dyn CancellationSignal,
    deadline: Instant,
) -> Result<Value, OperationFailure> {
    let directory = resolve_target(root, relative)?;
    if !directory.is_dir() {
        return Err(OperationFailure::Public(ToolPublicErrorCode::PathRejected));
    }

    let mut children = fs::read_dir(&directory)
        .map_err(map_io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_io)?;
    children.sort_by(|left, right| {
        left.file_name()
            .to_string_lossy()
            .cmp(&right.file_name().to_string_lossy())
    });

    let mut entries = Vec::new();
    let mut truncated = false;
    for child in children {
        checkpoint(cancellation, deadline)?;
        if entries.len() >= max_results as usize {
            truncated = true;
            break;
        }
        let metadata = child.path().symlink_metadata().map_err(map_io)?;
        let kind = if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            ToolEntryKind::Symlink
        } else if metadata.is_file() {
            ToolEntryKind::File
        } else if metadata.is_dir() {
            ToolEntryKind::Directory
        } else {
            ToolEntryKind::Other
        };
        let entry = FsListEntry {
            path: portable_relative(root, &child.path())?,
            kind,
            size_bytes: metadata.is_file().then_some(metadata.len()),
        };
        entries.push(entry);
        let candidate = FsListOutput {
            entries: entries.clone(),
            truncated: true,
        };
        if measured_json_bytes(&candidate)? > output_byte_cap {
            entries.pop();
            truncated = true;
            break;
        }
    }

    serde_json::to_value(FsListOutput { entries, truncated })
        .map_err(|_| OperationFailure::Public(ToolPublicErrorCode::InternalError))
}

fn execute_read(
    root: &Path,
    relative: &str,
    offset_bytes: u64,
    max_bytes: u64,
    output_byte_cap: u64,
    cancellation: &dyn CancellationSignal,
    deadline: Instant,
) -> Result<Value, OperationFailure> {
    let path = resolve_target(root, relative)?;
    if !path.is_file() {
        return Err(OperationFailure::Public(ToolPublicErrorCode::PathRejected));
    }
    checkpoint(cancellation, deadline)?;

    let mut file = File::open(&path).map_err(map_io)?;
    if resolve_target(root, relative)? != path {
        return Err(OperationFailure::Public(ToolPublicErrorCode::PathRejected));
    }
    let length = file.metadata().map_err(map_io)?.len();
    file.seek(SeekFrom::Start(offset_bytes)).map_err(map_io)?;
    let mut bytes = Vec::with_capacity(max_bytes.min(65_536) as usize);
    file.by_ref()
        .take(max_bytes)
        .read_to_end(&mut bytes)
        .map_err(map_io)?;
    checkpoint(cancellation, deadline)?;

    let truncated = length > offset_bytes.saturating_add(bytes.len() as u64);
    let (content_kind, content, bytes_returned) = match String::from_utf8(bytes) {
        Ok(content) => {
            let length = content.len() as u64;
            (ToolContentKind::Utf8, Some(content), length)
        }
        Err(_) => (ToolContentKind::Binary, None, 0),
    };
    let output = FsReadOutput {
        path: relative.to_owned(),
        content_kind,
        content,
        bytes_returned,
        truncated,
    };
    if measured_json_bytes(&output)? > output_byte_cap {
        return Err(OperationFailure::Public(
            ToolPublicErrorCode::OutputLimitExceeded,
        ));
    }
    serde_json::to_value(output)
        .map_err(|_| OperationFailure::Public(ToolPublicErrorCode::InternalError))
}

#[allow(clippy::too_many_arguments)]
fn execute_search(
    root: &Path,
    relative: &str,
    query: &str,
    case_sensitive: bool,
    max_results: u32,
    output_byte_cap: u64,
    cancellation: &dyn CancellationSignal,
    deadline: Instant,
) -> Result<Value, OperationFailure> {
    let target = resolve_target(root, relative)?;
    let mut files = Vec::new();
    collect_files(root, &target, &mut files, cancellation, deadline)?;
    files.sort_by_key(|path| path.to_string_lossy().to_string());

    let needle = if case_sensitive {
        query.to_owned()
    } else {
        query.to_lowercase()
    };
    let mut matches = Vec::new();
    let mut truncated = false;

    'files: for path in files {
        checkpoint(cancellation, deadline)?;
        let file = File::open(&path).map_err(map_io)?;
        let portable_path = portable_relative(root, &path)?;
        if resolve_target(root, &portable_path)? != path {
            return Err(OperationFailure::Public(ToolPublicErrorCode::PathRejected));
        }
        let mut reader = BufReader::new(file);
        let mut line_number = 0_u64;
        while let Some((mut bytes, overflowed)) =
            read_bounded_line(&mut reader, cancellation, deadline)?
        {
            line_number += 1;
            if overflowed {
                continue;
            }
            while matches!(bytes.last(), Some(b'\r')) {
                bytes.pop();
            }
            let Ok(line) = std::str::from_utf8(&bytes) else {
                continue;
            };
            let haystack = if case_sensitive {
                line.to_owned()
            } else {
                line.to_lowercase()
            };
            let mut start = 0;
            while let Some(found) = haystack[start..].find(&needle) {
                if matches.len() >= max_results as usize {
                    truncated = true;
                    break 'files;
                }
                let byte_index = start + found;
                let preview = bounded_preview(line);
                matches.push(FsSearchMatch {
                    path: portable_path.clone(),
                    line: line_number,
                    column: byte_index as u64 + 1,
                    preview,
                });
                let candidate = FsSearchOutput {
                    matches: matches.clone(),
                    truncated: true,
                };
                if measured_json_bytes(&candidate)? > output_byte_cap {
                    matches.pop();
                    truncated = true;
                    break 'files;
                }
                start = byte_index + needle.len();
            }
        }
    }

    serde_json::to_value(FsSearchOutput { matches, truncated })
        .map_err(|_| OperationFailure::Public(ToolPublicErrorCode::InternalError))
}

fn collect_files(
    root: &Path,
    target: &Path,
    output: &mut Vec<PathBuf>,
    cancellation: &dyn CancellationSignal,
    deadline: Instant,
) -> Result<(), OperationFailure> {
    checkpoint(cancellation, deadline)?;
    let metadata = target.symlink_metadata().map_err(map_io)?;
    if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        return Err(OperationFailure::Public(ToolPublicErrorCode::PathRejected));
    }
    if metadata.is_file() {
        if output.len() >= MAX_SEARCH_FILES {
            return Err(OperationFailure::Public(
                ToolPublicErrorCode::OutputLimitExceeded,
            ));
        }
        output.push(target.to_owned());
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(OperationFailure::Public(ToolPublicErrorCode::PathRejected));
    }

    let mut children = fs::read_dir(target)
        .map_err(map_io)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_io)?;
    children.sort_by(|left, right| {
        left.file_name()
            .to_string_lossy()
            .cmp(&right.file_name().to_string_lossy())
    });
    for child in children {
        checkpoint(cancellation, deadline)?;
        let path = child.path();
        let metadata = path.symlink_metadata().map_err(map_io)?;
        if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            continue;
        }
        let canonical = path.canonicalize().map_err(map_io)?;
        if !canonical.starts_with(root) {
            return Err(OperationFailure::Public(ToolPublicErrorCode::PathRejected));
        }
        collect_files(root, &canonical, output, cancellation, deadline)?;
    }
    Ok(())
}

fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    cancellation: &dyn CancellationSignal,
    deadline: Instant,
) -> Result<Option<(Vec<u8>, bool)>, OperationFailure> {
    let mut output = Vec::new();
    let mut saw_data = false;
    let mut overflowed = false;
    loop {
        checkpoint(cancellation, deadline)?;
        let buffer = reader.fill_buf().map_err(map_io)?;
        if buffer.is_empty() {
            return if saw_data {
                Ok(Some((output, overflowed)))
            } else {
                Ok(None)
            };
        }
        saw_data = true;
        let newline = buffer.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(buffer.len(), |index| index + 1);
        let content_length = newline.unwrap_or(buffer.len());
        if !overflowed {
            let remaining = MAX_SEARCH_LINE_BYTES.saturating_sub(output.len());
            let copied = content_length.min(remaining);
            output.extend_from_slice(&buffer[..copied]);
            if copied < content_length
                || (newline.is_none() && output.len() == MAX_SEARCH_LINE_BYTES)
            {
                overflowed = true;
            }
        }
        reader.consume(consumed);
        if newline.is_some() {
            return Ok(Some((output, overflowed)));
        }
    }
}

fn bounded_preview(line: &str) -> String {
    let mut preview = String::new();
    for character in line.chars() {
        let character = if character.is_control() && character != '\t' {
            ' '
        } else {
            character
        };
        if preview.len() + character.len_utf8() > MAX_SEARCH_PREVIEW_BYTES {
            break;
        }
        preview.push(character);
    }
    preview
}

fn resolve_target(root: &Path, relative: &str) -> Result<PathBuf, OperationFailure> {
    let mut current = root.to_owned();
    if relative != "." {
        for component in relative.split('/') {
            current.push(component);
            let metadata = current.symlink_metadata().map_err(map_path_io)?;
            if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
                return Err(OperationFailure::Public(ToolPublicErrorCode::PathRejected));
            }
            let canonical = current.canonicalize().map_err(map_path_io)?;
            if !canonical.starts_with(root) {
                return Err(OperationFailure::Public(ToolPublicErrorCode::PathRejected));
            }
            current = canonical;
        }
    }
    Ok(current)
}

fn portable_relative(root: &Path, path: &Path) -> Result<String, OperationFailure> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| OperationFailure::Public(ToolPublicErrorCode::PathRejected))?;
    let mut parts = Vec::new();
    for component in relative.components() {
        let text = component
            .as_os_str()
            .to_str()
            .ok_or(OperationFailure::Public(ToolPublicErrorCode::PathRejected))?;
        parts.push(text);
    }
    Ok(if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    })
}

fn measured_json_bytes<T: serde::Serialize>(value: &T) -> Result<u64, OperationFailure> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len() as u64)
        .map_err(|_| OperationFailure::Public(ToolPublicErrorCode::InternalError))
}

fn map_io(_: std::io::Error) -> OperationFailure {
    OperationFailure::Public(ToolPublicErrorCode::InternalError)
}

fn map_path_io(_: std::io::Error) -> OperationFailure {
    OperationFailure::Public(ToolPublicErrorCode::PathRejected)
}

#[cfg(windows)]
fn is_reparse_point(metadata: &Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_: &Metadata) -> bool {
    false
}
