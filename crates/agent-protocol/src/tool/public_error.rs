use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolPublicError {
    code: ToolPublicErrorCode,
}

impl ToolPublicError {
    pub fn canonical(code: ToolPublicErrorCode) -> Self {
        Self { code }
    }

    pub fn code(self) -> ToolPublicErrorCode {
        self.code
    }

    pub fn message(self) -> &'static str {
        match self.code {
            ToolPublicErrorCode::InvalidProposal => {
                "The tool request was rejected. Correct the arguments and try again."
            }
            ToolPublicErrorCode::ProjectMismatch => {
                "The tool request does not belong to the active project."
            }
            ToolPublicErrorCode::PathRejected => {
                "The requested project-relative path was rejected."
            }
            ToolPublicErrorCode::Cancelled => "The tool call was cancelled.",
            ToolPublicErrorCode::DeadlineExceeded => "The tool call exceeded its time limit.",
            ToolPublicErrorCode::OutputLimitExceeded => {
                "The tool output exceeded its configured limit. Narrow the request and try again."
            }
            ToolPublicErrorCode::InternalError => "The tool call failed inside Keeper.",
        }
    }

    pub fn retryable(self) -> bool {
        !matches!(self.code, ToolPublicErrorCode::ProjectMismatch)
    }
}

impl Serialize for ToolPublicError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut value = serializer.serialize_struct("ToolPublicError", 3)?;
        value.serialize_field("code", &self.code())?;
        value.serialize_field("message", self.message())?;
        value.serialize_field("retryable", &self.retryable())?;
        value.end()
    }
}

impl<'de> Deserialize<'de> for ToolPublicError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct WireError {
            code: ToolPublicErrorCode,
            message: String,
            retryable: bool,
        }

        let wire = WireError::deserialize(deserializer)?;
        let canonical = Self::canonical(wire.code);
        if wire.message != canonical.message() || wire.retryable != canonical.retryable() {
            return Err(serde::de::Error::custom(
                "public tool error must use the canonical secret-safe representation",
            ));
        }
        Ok(canonical)
    }
}
