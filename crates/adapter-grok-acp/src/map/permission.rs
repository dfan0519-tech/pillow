//! B2 read-only permission policy for `session/request_permission`.

use std::collections::HashMap;

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
}

/// Session-local tool observation registry (never exported upward).
#[derive(Debug)]
pub struct ToolRegistry {
    session_id: String,
    calls: HashMap<String, CallState>,
    activity_count: usize,
    pub overflowed: bool,
}

impl ToolRegistry {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
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
    });
    if recognized || session_update == "tool_call" {
        entry.kind = kind;
        entry.recognized = recognized || is_known_kind(&entry.kind);
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
    if same_session
        && read_only
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

    #[test]
    fn allows_read_once_for_same_session() {
        let mut reg = ToolRegistry::new("sess-1");
        observe_tool_update(
            &mut reg,
            &json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "tc-1",
                "kind": "read",
                "title": "read_file"
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
    fn denies_write_even_with_allow_always() {
        let mut reg = ToolRegistry::new("sess-1");
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
        let reg = ToolRegistry::new("sess-1");
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
