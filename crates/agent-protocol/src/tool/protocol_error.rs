use std::error::Error;
use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolProtocolErrorCode {
    InvalidEnvelope,
    UnknownTool,
    UnsupportedToolVersion,
    ActiveContextMismatch,
    InvalidArguments,
    ArgumentsDigestMismatch,
    InvalidResult,
}

impl ToolProtocolErrorCode {
    pub const fn message(self) -> &'static str {
        match self {
            Self::InvalidEnvelope => "The tool envelope is invalid.",
            Self::UnknownTool => "The tool is not in the fixed Keeper catalog.",
            Self::UnsupportedToolVersion => "The tool version is not supported.",
            Self::ActiveContextMismatch => {
                "The tool message does not belong to the active context."
            }
            Self::InvalidArguments => "Tool arguments do not match the fixed protocol contract.",
            Self::ArgumentsDigestMismatch => {
                "The tool arguments digest does not match normalized arguments."
            }
            Self::InvalidResult => "The tool result does not match the validated proposal.",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolProtocolError {
    code: ToolProtocolErrorCode,
}

impl ToolProtocolError {
    pub(crate) const fn new(code: ToolProtocolErrorCode) -> Self {
        Self { code }
    }

    pub const fn code(self) -> ToolProtocolErrorCode {
        self.code
    }

    pub const fn message(self) -> &'static str {
        self.code.message()
    }
}

impl fmt::Display for ToolProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.code, self.message())
    }
}

impl Error for ToolProtocolError {}
