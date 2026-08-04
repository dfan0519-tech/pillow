use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use super::protocol_error::{ToolProtocolError, ToolProtocolErrorCode};
use super::public_error::ToolPublicError;
use super::reservation::{ToolCallReservation, ToolProposalFingerprint};
use crate::RunId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArgumentsDigest(String);

impl ArgumentsDigest {
    pub fn parse(value: impl Into<String>) -> Result<Self, ToolProtocolError> {
        let value = value.into();
        let Some(hex) = value.strip_prefix("sha256:") else {
            return Err(ToolProtocolError::new(
                ToolProtocolErrorCode::InvalidArguments,
            ));
        };
        if hex.len() != 64
            || !hex
                .as_bytes()
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
        {
            return Err(ToolProtocolError::new(
                ToolProtocolErrorCode::InvalidArguments,
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn from_sha256_hex(hex: String) -> Self {
        Self(format!("sha256:{hex}"))
    }
}

impl Serialize for ArgumentsDigest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ArgumentsDigest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(value).map_err(serde::de::Error::custom)
    }
}

/// Maximum UTF-8 byte length for `fs.write@1` content (C3.0).
pub const WRITE_CONTENT_BYTE_CAP: u64 = 262_144;

/// Maximum number of argv entries for `process.run@1` (C4.0).
pub const PROCESS_ARGV_COUNT_CAP: usize = 64;

/// Maximum combined UTF-8 byte length of program label plus all argv entries.
pub const PROCESS_ARGV_BYTE_CAP: usize = 32_768;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolId {
    #[serde(rename = "fs.list")]
    FsList,
    #[serde(rename = "fs.read")]
    FsRead,
    #[serde(rename = "fs.search")]
    FsSearch,
    #[serde(rename = "fs.write")]
    FsWrite,
    #[serde(rename = "process.run")]
    ProcessRun,
}

impl ToolId {
    pub const fn all() -> &'static [Self] {
        &[
            Self::FsList,
            Self::FsRead,
            Self::FsSearch,
            Self::FsWrite,
            Self::ProcessRun,
        ]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FsList => "fs.list",
            Self::FsRead => "fs.read",
            Self::FsSearch => "fs.search",
            Self::FsWrite => "fs.write",
            Self::ProcessRun => "process.run",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "fs.list" => Some(Self::FsList),
            "fs.read" => Some(Self::FsRead),
            "fs.search" => Some(Self::FsSearch),
            "fs.write" => Some(Self::FsWrite),
            "process.run" => Some(Self::ProcessRun),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteMode {
    Create,
    Modify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolArgumentType {
    ProjectRelativePath,
    String,
    StringArray,
    Bool,
    U32,
    U64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolArgumentSpec {
    pub name: String,
    pub value_type: ToolArgumentType,
    pub required: bool,
    pub default: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolExecutionLimits {
    pub output_byte_cap: u64,
    pub result_count_cap: Option<u32>,
    pub timeout_ms: u64,
    pub cancellation_checks_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolManifest {
    pub tool_id: ToolId,
    pub tool_version: u16,
    pub description: String,
    pub arguments: Vec<ToolArgumentSpec>,
    pub limits: ToolExecutionLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
/// Provider-neutral wire proposal.
///
/// Operational callers must use [`ToolProposal::parse_str`],
/// [`ToolProposal::parse_bytes`], or [`ToolProposal::parse_value`] so malformed
/// provider input cannot expose serde diagnostics.
///
/// ```compile_fail
/// use keeper_agent_protocol::ToolProposal;
///
/// let _: ToolProposal = serde_json::from_str("{}").unwrap();
/// ```
pub struct ToolProposal {
    pub project_id: String,
    pub conversation_id: String,
    pub run_id: RunId,
    pub call_id: String,
    pub tool_id: String,
    pub tool_version: u16,
    pub arguments: Value,
    pub arguments_digest: ArgumentsDigest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolActiveContext {
    pub(super) project_id: String,
    pub(super) conversation_id: String,
    pub(super) run_id: RunId,
}

impl ToolActiveContext {
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    pub fn conversation_id(&self) -> &str {
        &self.conversation_id
    }

    pub fn run_id(&self) -> RunId {
        self.run_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A proposal that passed strict context, schema, digest, and identity validation.
///
/// Its fields are intentionally opaque so callers cannot forge validation.
///
/// ```compile_fail
/// use keeper_agent_protocol::ValidatedToolProposal;
///
/// let value: ValidatedToolProposal = todo!();
/// let ValidatedToolProposal {
///     proposal,
///     manifest,
///     arguments,
///     canonical_arguments,
///     ..
/// } = value;
/// ```
pub struct ValidatedToolProposal {
    pub(super) proposal: ToolProposal,
    pub(super) context: ToolActiveContext,
    pub(super) manifest: ToolManifest,
    pub(super) arguments: ToolArguments,
    pub(super) canonical_arguments: Value,
    pub(super) normalized_arguments_digest: ArgumentsDigest,
    pub(super) reservation: ToolCallReservation,
}

impl ValidatedToolProposal {
    pub fn proposal(&self) -> &ToolProposal {
        &self.proposal
    }

    pub fn context(&self) -> &ToolActiveContext {
        &self.context
    }

    pub fn manifest(&self) -> &ToolManifest {
        &self.manifest
    }

    pub fn arguments(&self) -> &ToolArguments {
        &self.arguments
    }

    pub fn canonical_arguments(&self) -> &Value {
        &self.canonical_arguments
    }

    pub fn normalized_arguments_digest(&self) -> &ArgumentsDigest {
        &self.normalized_arguments_digest
    }

    /// Stable semantic fingerprint for this validated proposal.
    pub fn fingerprint(&self) -> &ToolProposalFingerprint {
        self.reservation.fingerprint()
    }

    /// Call identity and proposal fingerprint to reserve before execution.
    ///
    /// The caller must atomically apply the reservation classification. This
    /// crate intentionally does not implement reservation persistence.
    pub fn reservation(&self) -> &ToolCallReservation {
        &self.reservation
    }

    pub fn project_id(&self) -> &str {
        &self.proposal.project_id
    }

    pub fn conversation_id(&self) -> &str {
        &self.proposal.conversation_id
    }

    pub fn run_id(&self) -> RunId {
        self.proposal.run_id
    }

    pub fn call_id(&self) -> &str {
        &self.proposal.call_id
    }

    pub fn tool_id(&self) -> &str {
        &self.proposal.tool_id
    }

    pub fn tool_version(&self) -> u16 {
        self.proposal.tool_version
    }
}

fn default_list_results() -> u32 {
    100
}

fn default_read_bytes() -> u64 {
    65_536
}

fn default_search_path() -> String {
    ".".into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsListArguments {
    pub path: String,
    #[serde(default = "default_list_results")]
    pub max_results: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsReadArguments {
    pub path: String,
    #[serde(default)]
    pub offset_bytes: u64,
    #[serde(default = "default_read_bytes")]
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsSearchArguments {
    #[serde(default = "default_search_path")]
    pub path: String,
    pub query: String,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default = "default_list_results")]
    pub max_results: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsWriteArguments {
    pub path: String,
    pub mode: WriteMode,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessRunArguments {
    pub program: String,
    pub argv: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolArguments {
    FsList(FsListArguments),
    FsRead(FsReadArguments),
    FsSearch(FsSearchArguments),
    FsWrite(FsWriteArguments),
    ProcessRun(ProcessRunArguments),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionState {
    Proposed,
    Validated,
    Executing,
    Succeeded,
    Failed,
    Cancelled,
}

impl ToolExecutionState {
    pub const fn all() -> &'static [Self] {
        &[
            Self::Proposed,
            Self::Validated,
            Self::Executing,
            Self::Succeeded,
            Self::Failed,
            Self::Cancelled,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolEntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsListEntry {
    pub path: String,
    pub kind: ToolEntryKind,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsListOutput {
    pub entries: Vec<FsListEntry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolContentKind {
    Utf8,
    Binary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsReadOutput {
    pub path: String,
    pub content_kind: ToolContentKind,
    pub content: Option<String>,
    pub bytes_returned: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsSearchMatch {
    pub path: String,
    pub line: u64,
    pub column: u64,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsSearchOutput {
    pub matches: Vec<FsSearchMatch>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsWriteOutput {
    pub path: String,
    pub mode: WriteMode,
    pub bytes_written: u64,
    pub content_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessRunOutput {
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub stdout_sha256: String,
    pub stderr_sha256: String,
    pub captured_bytes: u64,
    pub truncated: bool,
    pub encoding_lossy: bool,
    pub cancelled: bool,
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
/// Unvalidated tool result wire data.
///
/// Operational callers must use [`ToolResult::parse_str`],
/// [`ToolResult::parse_bytes`], or [`ToolResult::parse_value`] and then validate
/// against the owning proposal.
///
/// ```compile_fail
/// use keeper_agent_protocol::ToolResult;
///
/// let _: ToolResult = serde_json::from_str("{}").unwrap();
/// ```
pub struct ToolResult {
    pub project_id: String,
    pub conversation_id: String,
    pub run_id: RunId,
    pub call_id: String,
    pub tool_id: String,
    pub tool_version: u16,
    pub proposal_fingerprint: ToolProposalFingerprint,
    pub arguments_digest: ArgumentsDigest,
    pub state: ToolExecutionState,
    pub output: Option<Value>,
    pub error: Option<ToolPublicError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A tool result that is strictly bound to one validated proposal and context.
///
/// Fields are private and all accessors are read-only, so validation cannot be
/// forged or mutated by callers.
///
/// ```compile_fail
/// use keeper_agent_protocol::ValidatedToolResult;
///
/// let value: ValidatedToolResult = todo!();
/// let ValidatedToolResult { result, context } = value;
/// ```
///
/// ```compile_fail
/// use keeper_agent_protocol::ValidatedToolResult;
///
/// fn mutate(value: &mut ValidatedToolResult) {
///     value.result().state = keeper_agent_protocol::ToolExecutionState::Failed;
/// }
/// ```
pub struct ValidatedToolResult {
    pub(super) result: ToolResult,
    pub(super) context: ToolActiveContext,
}

impl ValidatedToolResult {
    pub fn result(&self) -> &ToolResult {
        &self.result
    }

    pub fn context(&self) -> &ToolActiveContext {
        &self.context
    }

    pub fn project_id(&self) -> &str {
        &self.result.project_id
    }

    pub fn conversation_id(&self) -> &str {
        &self.result.conversation_id
    }

    pub fn run_id(&self) -> RunId {
        self.result.run_id
    }

    pub fn call_id(&self) -> &str {
        &self.result.call_id
    }

    pub fn tool_id(&self) -> &str {
        &self.result.tool_id
    }

    pub fn tool_version(&self) -> u16 {
        self.result.tool_version
    }

    pub fn proposal_fingerprint(&self) -> &ToolProposalFingerprint {
        &self.result.proposal_fingerprint
    }

    pub fn normalized_arguments_digest(&self) -> &ArgumentsDigest {
        &self.result.arguments_digest
    }

    pub fn state(&self) -> ToolExecutionState {
        self.result.state
    }

    pub fn output(&self) -> Option<&Value> {
        self.result.output.as_ref()
    }

    pub fn error(&self) -> Option<ToolPublicError> {
        self.result.error
    }
}
