//! B2 read-only permission policy for `session/request_permission`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config::DEFAULT_MAX_TOOL_ACTIVITY;
use crate::rpc::permission_result;

const READ_ONLY_KINDS: &[&str] = &["read", "search"];
const KNOWN_KINDS: &[&str] = &[
    "read",
    "edit",
    "write",
    "delete",
    "move",
    "search",
    "execute",
    "think",
    "fetch",
    "other",
    "network",
    "credential",
];

#[derive(Debug, Clone)]
struct CallState {
    kind: String,
    recognized: bool,
    tool_inputs: Vec<Value>,
    target_observation_overflowed: bool,
}

/// Session-local tool observation registry (never exported upward).
#[derive(Debug)]
pub struct ToolRegistry {
    session_id: String,
    project_root: Option<PathBuf>,
    calls: HashMap<String, CallState>,
    activity_count: usize,
    pub overflowed: bool,
}

impl ToolRegistry {
    pub fn new(session_id: impl Into<String>, project_root: impl AsRef<Path>) -> Self {
        Self {
            session_id: session_id.into(),
            project_root: project_root
                .as_ref()
                .canonicalize()
                .ok()
                .filter(|path| path.is_dir()),
            calls: HashMap::new(),
            activity_count: 0,
            overflowed: false,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

/// Outcome of a permission decision (private to the adapter).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionDecision {
    pub result: Value,
    pub allowed: bool,
}

/// Observe a `tool_call` / `tool_call_update` so later permission can match kinds.
pub fn observe_tool_update(registry: &mut ToolRegistry, update: &Value) {
    let session_update = update
        .get("sessionUpdate")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if session_update != "tool_call" && session_update != "tool_call_update" {
        return;
    }
    let tool_call_id = safe_tool_call_id(update.get("toolCallId").and_then(|v| v.as_str()));
    if tool_call_id.is_empty() {
        return;
    }
    let mut kind = extract_kind(update);
    let mut recognized = is_known_kind(&kind);
    if kind.is_empty() {
        kind = "other".into();
        recognized = false;
    } else {
        kind = normalize_kind(&kind);
    }
    let entry = registry.calls.entry(tool_call_id).or_insert(CallState {
        kind: kind.clone(),
        recognized,
        tool_inputs: Vec::new(),
        target_observation_overflowed: false,
    });
    if recognized || session_update == "tool_call" {
        entry.kind = kind;
        entry.recognized = recognized || is_known_kind(&entry.kind);
    }
    if contains_target_field(update) {
        if entry.tool_inputs.len() >= 4 {
            entry.target_observation_overflowed = true;
        } else {
            entry.tool_inputs.push(update.clone());
        }
    }
    bump_activity(registry);
}

/// Decide allow_once for read/search; deny everything else.
pub fn decide_permission(
    registry: Option<&ToolRegistry>,
    params: &Value,
    cancelled: bool,
) -> PermissionDecision {
    if cancelled {
        return PermissionDecision {
            result: permission_result(None),
            allowed: false,
        };
    }

    let session_id = params
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let tool_call = params.get("toolCall").cloned().unwrap_or(Value::Null);
    let tool_call_id = safe_tool_call_id(tool_call.get("toolCallId").and_then(|v| v.as_str()));
    let options = params.get("options").and_then(|v| v.as_array());

    let same_session = registry
        .map(|r| r.session_id() == session_id)
        .unwrap_or(false);

    let mut kind = extract_kind(&tool_call);
    let mut recognized = is_known_kind(&kind);
    if let Some(reg) = registry
        && same_session
        && let Some(call) = reg.calls.get(&tool_call_id)
    {
        if call.recognized {
            kind = call.kind.clone();
            recognized = true;
        } else if kind.is_empty() {
            kind = call.kind.clone();
            recognized = call.recognized;
        }
    }
    if !kind.is_empty() {
        kind = normalize_kind(&kind);
        recognized = recognized || is_known_kind(&kind);
    }

    let allow_once = option_id_by_kinds(options, &["allow_once"]);
    let reject = option_id_by_kinds(options, &["reject_once", "reject_always"]);

    let read_only = recognized && READ_ONLY_KINDS.contains(&kind.as_str());
    let target_is_in_project = registry
        .filter(|_| same_session)
        .and_then(|reg| reg.calls.get(&tool_call_id).map(|call| (reg, call)))
        .is_some_and(|(reg, call)| {
            !call.target_observation_overflowed
                && reg
                    .project_root
                    .as_deref()
                    .is_some_and(|root| all_targets_are_inside(root, &call.tool_inputs, &tool_call))
        });
    if same_session
        && read_only
        && target_is_in_project
        && let Some(option_id) = allow_once
    {
        return PermissionDecision {
            result: permission_result(Some(option_id)),
            allowed: true,
        };
    }

    // Never select allow_always for any class in B2.
    PermissionDecision {
        result: permission_result(reject),
        allowed: false,
    }
}

fn contains_target_field(value: &Value) -> bool {
    match value {
        Value::Object(object) => object
            .iter()
            .any(|(key, value)| is_target_field(key) || contains_target_field(value)),
        Value::Array(values) => values.iter().any(contains_target_field),
        _ => false,
    }
}

fn all_targets_are_inside(root: &Path, observed: &[Value], permission_call: &Value) -> bool {
    let mut targets = Vec::new();
    let mut saw_target = false;
    let mut ambiguous = false;
    for value in observed.iter().chain(std::iter::once(permission_call)) {
        collect_targets(value, &mut targets, &mut saw_target, &mut ambiguous);
    }
    saw_target
        && !ambiguous
        && !targets.is_empty()
        && targets
            .iter()
            .all(|target| target_is_inside_project(root, target))
}

fn collect_targets(
    value: &Value,
    targets: &mut Vec<String>,
    saw_target: &mut bool,
    ambiguous: &mut bool,
) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if is_target_field(key) {
                    *saw_target = true;
                    collect_target_value(value, targets, ambiguous);
                }
                collect_targets(value, targets, saw_target, ambiguous);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_targets(value, targets, saw_target, ambiguous);
            }
        }
        _ => {}
    }
}

fn collect_target_value(value: &Value, targets: &mut Vec<String>, ambiguous: &mut bool) {
    match value {
        Value::String(target) if !target.trim().is_empty() => targets.push(target.clone()),
        Value::Array(values) if !values.is_empty() => {
            for value in values {
                match value {
                    Value::String(target) if !target.trim().is_empty() => {
                        targets.push(target.clone())
                    }
                    _ => *ambiguous = true,
                }
            }
        }
        _ => *ambiguous = true,
    }
}

fn is_target_field(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "target" | "path" | "cwd" | "root" | "location"
    )
}

fn target_is_inside_project(root: &Path, target: &str) -> bool {
    let target = target.trim();
    if target.is_empty() || has_disallowed_windows_path_syntax(target) {
        return false;
    }
    let path = Path::new(target);
    if has_windows_drive_prefix(target) && !path.is_absolute() {
        return false;
    }
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    resolved
        .canonicalize()
        .is_ok_and(|canonical| canonical.starts_with(root))
}

fn has_disallowed_windows_path_syntax(target: &str) -> bool {
    target.starts_with("\\\\")
        || target.starts_with("\\\\?\\")
        || target.starts_with("\\\\.\\")
        || target.starts_with("\\??\\")
        || target.starts_with('\\')
        || target.starts_with('/')
}

fn has_windows_drive_prefix(target: &str) -> bool {
    let bytes = target.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn extract_kind(obj: &Value) -> String {
    if let Some(k) = obj.get("kind").and_then(|v| v.as_str())
        && !k.is_empty()
    {
        return k.to_ascii_lowercase();
    }
    // Prefer _meta["x.ai/tool"].kind when present.
    if let Some(meta) = obj.get("_meta").and_then(|v| v.as_object()) {
        for (key, value) in meta {
            if (key == "x.ai/tool" || key.ends_with("/tool"))
                && let Some(k) = value.get("kind").and_then(|v| v.as_str())
            {
                return k.to_ascii_lowercase();
            }
        }
    }
    // Title heuristic for bare tool_call.
    if let Some(title) = obj.get("title").and_then(|v| v.as_str()) {
        let t = title.to_ascii_lowercase();
        if t.contains("read") {
            return "read".into();
        }
        if t.contains("search") || t.contains("grep") {
            return "search".into();
        }
        if t.contains("write") || t.contains("edit") {
            return "write".into();
        }
    }
    String::new()
}

fn normalize_kind(kind: &str) -> String {
    match kind {
        "write" => "edit".into(),
        "network" => "fetch".into(),
        "credential" => "other".into(),
        other => other.to_string(),
    }
}

fn is_known_kind(kind: &str) -> bool {
    if kind.is_empty() {
        return false;
    }
    let n = normalize_kind(kind);
    KNOWN_KINDS.contains(&kind) || KNOWN_KINDS.contains(&n.as_str())
}

fn safe_tool_call_id(value: Option<&str>) -> String {
    let id = value.unwrap_or("").trim();
    if id.is_empty() || id.len() > 200 || id.chars().any(|c| c.is_control()) {
        return String::new();
    }
    id.to_string()
}

fn option_id_by_kinds<'a>(options: Option<&'a Vec<Value>>, kinds: &[&str]) -> Option<&'a str> {
    let options = options?;
    for opt in options {
        let kind = opt
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let option_id = opt.get("optionId").and_then(|v| v.as_str()).unwrap_or("");
        if option_id.is_empty() {
            continue;
        }
        if kinds.contains(&kind.as_str()) {
            return Some(option_id);
        }
    }
    None
}

fn bump_activity(registry: &mut ToolRegistry) {
    registry.activity_count += 1;
    if registry.activity_count > DEFAULT_MAX_TOOL_ACTIVITY {
        registry.overflowed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read_options() -> Value {
        json!([
            { "optionId": "allow-once", "name": "Yes", "kind": "allow_once" },
            { "optionId": "reject-once", "name": "No", "kind": "reject_once" }
        ])
    }

    fn test_root() -> PathBuf {
        std::env::temp_dir().canonicalize().expect("temp directory")
    }

    #[test]
    fn allows_read_once_for_same_session() {
        let mut reg = ToolRegistry::new("sess-1", test_root());
        observe_tool_update(
            &mut reg,
            &json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "tc-1",
                "kind": "read",
                "title": "read_file",
                "rawInput": { "target": "." }
            }),
        );
        let decision = decide_permission(
            Some(&reg),
            &json!({
                "sessionId": "sess-1",
                "toolCall": { "toolCallId": "tc-1", "kind": "read" },
                "options": read_options()
            }),
            false,
        );
        assert!(decision.allowed);
        assert_eq!(decision.result["outcome"]["optionId"], json!("allow-once"));
    }

    #[test]
    fn denies_read_without_a_structured_target() {
        let mut reg = ToolRegistry::new("sess-1", test_root());
        observe_tool_update(
            &mut reg,
            &json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "tc-missing-target",
                "kind": "read"
            }),
        );
        let decision = decide_permission(
            Some(&reg),
            &json!({
                "sessionId": "sess-1",
                "toolCall": { "toolCallId": "tc-missing-target", "kind": "read" },
                "options": read_options()
            }),
            false,
        );
        assert!(!decision.allowed);
        assert_eq!(decision.result["outcome"]["optionId"], json!("reject-once"));
    }

    #[test]
    fn denies_read_with_an_explicit_parent_target() {
        let mut reg = ToolRegistry::new("sess-1", test_root());
        observe_tool_update(
            &mut reg,
            &json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "tc-parent-target",
                "kind": "read",
                "rawInput": { "target": "../outside.txt" }
            }),
        );
        let decision = decide_permission(
            Some(&reg),
            &json!({
                "sessionId": "sess-1",
                "toolCall": { "toolCallId": "tc-parent-target", "kind": "read" },
                "options": read_options()
            }),
            false,
        );
        assert!(!decision.allowed);
        assert_eq!(decision.result["outcome"]["optionId"], json!("reject-once"));
    }

    #[test]
    fn allows_every_named_target_field_inside_the_project_root() {
        let mut reg = ToolRegistry::new("sess-1", test_root());
        observe_tool_update(
            &mut reg,
            &json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "tc-all-fields",
                "kind": "search",
                "rawInput": {
                    "target": ".",
                    "path": ".",
                    "cwd": ".",
                    "root": ".",
                    "location": "."
                }
            }),
        );
        let decision = decide_permission(
            Some(&reg),
            &json!({
                "sessionId": "sess-1",
                "toolCall": { "toolCallId": "tc-all-fields", "kind": "search" },
                "options": read_options()
            }),
            false,
        );
        assert!(decision.allowed);
    }

    #[test]
    fn denies_any_ambiguous_or_outside_target() {
        let root = test_root();
        let outside = root.parent().expect("temp parent").to_string_lossy();
        for (case, target) in [
            ("missing", json!("not-a-real-target")),
            ("absolute", json!(outside)),
            ("unc", json!(r"\\server\share\file")),
            ("device", json!(r"\\?\C:\device\file")),
            ("ambiguous", json!({ "path": "." })),
        ] {
            let mut reg = ToolRegistry::new("sess-1", &root);
            observe_tool_update(
                &mut reg,
                &json!({
                    "sessionUpdate": "tool_call",
                    "toolCallId": format!("tc-{case}"),
                    "kind": "read",
                    "rawInput": { "target": target }
                }),
            );
            let decision = decide_permission(
                Some(&reg),
                &json!({
                    "sessionId": "sess-1",
                    "toolCall": { "toolCallId": format!("tc-{case}"), "kind": "read" },
                    "options": read_options()
                }),
                false,
            );
            assert!(!decision.allowed, "{case} target must be rejected");
        }
    }

    #[test]
    fn denies_read_when_any_discovered_target_leaves_the_project_root() {
        let mut reg = ToolRegistry::new("sess-1", test_root());
        observe_tool_update(
            &mut reg,
            &json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "tc-mixed-targets",
                "kind": "read",
                "rawInput": { "target": ".", "location": ".." }
            }),
        );
        let decision = decide_permission(
            Some(&reg),
            &json!({
                "sessionId": "sess-1",
                "toolCall": { "toolCallId": "tc-mixed-targets", "kind": "read" },
                "options": read_options()
            }),
            false,
        );
        assert!(!decision.allowed);
    }

    #[test]
    fn denies_write_even_with_allow_always() {
        let mut reg = ToolRegistry::new("sess-1", test_root());
        observe_tool_update(
            &mut reg,
            &json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "tc-w",
                "kind": "edit",
                "title": "write"
            }),
        );
        let decision = decide_permission(
            Some(&reg),
            &json!({
                "sessionId": "sess-1",
                "toolCall": { "toolCallId": "tc-w", "kind": "edit" },
                "options": [
                    { "optionId": "allow-edits-session", "kind": "allow_always" },
                    { "optionId": "allow-once", "kind": "allow_once" },
                    { "optionId": "reject-once", "kind": "reject_once" }
                ]
            }),
            false,
        );
        assert!(!decision.allowed);
        assert_eq!(decision.result["outcome"]["optionId"], json!("reject-once"));
    }

    #[test]
    fn denies_unknown_and_foreign_session() {
        let reg = ToolRegistry::new("sess-1", test_root());
        let decision = decide_permission(
            Some(&reg),
            &json!({
                "sessionId": "other",
                "toolCall": { "toolCallId": "tc-x", "kind": "read" },
                "options": read_options()
            }),
            false,
        );
        assert!(!decision.allowed);
    }

    #[test]
    fn cancelled_prompt_denies() {
        let decision = decide_permission(None, &json!({}), true);
        assert!(!decision.allowed);
        assert_eq!(decision.result["outcome"]["outcome"], json!("cancelled"));
    }
}
