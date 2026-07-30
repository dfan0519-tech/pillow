use serde_json::Value;

use super::types::{ToolArgumentSpec, ToolArgumentType, ToolExecutionLimits, ToolId, ToolManifest};

pub(crate) const LIST_OUTPUT_BYTE_CAP: u64 = 262_144;
pub(crate) const LIST_RESULT_COUNT_CAP: u32 = 1_000;
pub(crate) const READ_OUTPUT_BYTE_CAP: u64 = 1_048_576;
pub(crate) const SEARCH_OUTPUT_BYTE_CAP: u64 = 524_288;
pub(crate) const SEARCH_RESULT_COUNT_CAP: u32 = 500;

pub fn fixed_tool_manifests() -> Vec<ToolManifest> {
    vec![
        ToolManifest {
            tool_id: ToolId::FsList,
            tool_version: 1,
            description:
                "List entries under one project-relative directory without following links.".into(),
            arguments: vec![
                argument(
                    "path",
                    ToolArgumentType::ProjectRelativePath,
                    true,
                    Value::Null,
                ),
                argument(
                    "max_results",
                    ToolArgumentType::U32,
                    false,
                    Value::from(100),
                ),
            ],
            limits: limits(LIST_OUTPUT_BYTE_CAP, Some(LIST_RESULT_COUNT_CAP), 2_000),
        },
        ToolManifest {
            tool_id: ToolId::FsRead,
            tool_version: 1,
            description: "Read a bounded byte range from one project-relative file.".into(),
            arguments: vec![
                argument(
                    "path",
                    ToolArgumentType::ProjectRelativePath,
                    true,
                    Value::Null,
                ),
                argument("offset_bytes", ToolArgumentType::U64, false, Value::from(0)),
                argument(
                    "max_bytes",
                    ToolArgumentType::U64,
                    false,
                    Value::from(65_536),
                ),
            ],
            limits: limits(READ_OUTPUT_BYTE_CAP, None, 2_000),
        },
        ToolManifest {
            tool_id: ToolId::FsSearch,
            tool_version: 1,
            description:
                "Search UTF-8 project files for a literal query and return bounded matches.".into(),
            arguments: vec![
                argument(
                    "path",
                    ToolArgumentType::ProjectRelativePath,
                    false,
                    Value::String(".".into()),
                ),
                argument("query", ToolArgumentType::String, true, Value::Null),
                argument(
                    "case_sensitive",
                    ToolArgumentType::Bool,
                    false,
                    Value::Bool(false),
                ),
                argument(
                    "max_results",
                    ToolArgumentType::U32,
                    false,
                    Value::from(100),
                ),
            ],
            limits: limits(SEARCH_OUTPUT_BYTE_CAP, Some(SEARCH_RESULT_COUNT_CAP), 5_000),
        },
    ]
}

pub(crate) fn manifest_for(tool_id: ToolId) -> ToolManifest {
    fixed_tool_manifests()
        .into_iter()
        .find(|manifest| manifest.tool_id == tool_id)
        .expect("fixed manifest must exist for every ToolId")
}

fn argument(
    name: &str,
    value_type: ToolArgumentType,
    required: bool,
    default: Value,
) -> ToolArgumentSpec {
    ToolArgumentSpec {
        name: name.into(),
        value_type,
        required,
        default,
    }
}

fn limits(
    output_byte_cap: u64,
    result_count_cap: Option<u32>,
    timeout_ms: u64,
) -> ToolExecutionLimits {
    ToolExecutionLimits {
        output_byte_cap,
        result_count_cap,
        timeout_ms,
        cancellation_checks_required: true,
    }
}
