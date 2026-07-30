use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use thiserror::Error;

use crate::RunId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArgumentsDigest(String);

impl ArgumentsDigest {
    pub fn parse(value: impl Into<String>) -> Result<Self, ToolProtocolError> {
        let value = value.into();
        let Some(hex) = value.strip_prefix("sha256:") else {
            return Err(ToolProtocolError::new(
                ToolProtocolErrorCode::InvalidArguments,
                "arguments_digest must use the sha256:<lowercase-hex> format",
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
                "arguments_digest must contain exactly 64 lowercase hexadecimal characters",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolId {
    #[serde(rename = "fs.list")]
    FsList,
    #[serde(rename = "fs.read")]
    FsRead,
    #[serde(rename = "fs.search")]
    FsSearch,
}

impl ToolId {
    pub const fn all() -> &'static [Self] {
        &[Self::FsList, Self::FsRead, Self::FsSearch]
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FsList => "fs.list",
            Self::FsRead => "fs.read",
            Self::FsSearch => "fs.search",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "fs.list" => Some(Self::FsList),
            "fs.read" => Some(Self::FsRead),
            "fs.search" => Some(Self::FsSearch),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolArgumentType {
    ProjectRelativePath,
    String,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
pub struct ValidatedToolProposal {
    pub proposal: ToolProposal,
    pub manifest: ToolManifest,
    pub arguments: ToolArguments,
    pub canonical_arguments: Value,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolArguments {
    FsList(FsListArguments),
    FsRead(FsReadArguments),
    FsSearch(FsSearchArguments),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolPublicErrorCode {
    InvalidProposal,
    ProjectMismatch,
    PathRejected,
    Cancelled,
    DeadlineExceeded,
    OutputLimitExceeded,
    InternalError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolPublicError {
    pub code: ToolPublicErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl ToolPublicError {
    pub fn canonical(code: ToolPublicErrorCode) -> Self {
        let (message, retryable) = match code {
            ToolPublicErrorCode::InvalidProposal => (
                "The tool request was rejected. Correct the arguments and try again.",
                true,
            ),
            ToolPublicErrorCode::ProjectMismatch => (
                "The tool request does not belong to the active project.",
                false,
            ),
            ToolPublicErrorCode::PathRejected => {
                ("The requested project-relative path was rejected.", true)
            }
            ToolPublicErrorCode::Cancelled => ("The tool call was cancelled.", true),
            ToolPublicErrorCode::DeadlineExceeded => {
                ("The tool call exceeded its time limit.", true)
            }
            ToolPublicErrorCode::OutputLimitExceeded => (
                "The tool output exceeded its configured limit. Narrow the request and try again.",
                true,
            ),
            ToolPublicErrorCode::InternalError => ("The tool call failed inside Keeper.", true),
        };
        Self {
            code,
            message: message.into(),
            retryable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolResult {
    pub project_id: String,
    pub conversation_id: String,
    pub run_id: RunId,
    pub call_id: String,
    pub tool_id: String,
    pub tool_version: u16,
    pub state: ToolExecutionState,
    pub output: Option<Value>,
    pub error: Option<ToolPublicError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolProtocolErrorCode {
    InvalidEnvelope,
    UnknownTool,
    UnsupportedToolVersion,
    ProjectMismatch,
    InvalidArguments,
    ArgumentsDigestMismatch,
    InvalidResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{code:?}: {message}")]
pub struct ToolProtocolError {
    pub code: ToolProtocolErrorCode,
    pub message: String,
}

impl ToolProtocolError {
    pub(crate) fn new(code: ToolProtocolErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
