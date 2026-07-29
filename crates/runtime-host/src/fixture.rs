//! JSONL fixture format for the deterministic fake adapter.

use std::path::Path;

use keeper_agent_protocol::{RuntimeError, RuntimeEventKind};
use serde::Deserialize;
use serde_json::Value;

/// Committed fixture embedded for deterministic in-process replay.
pub const FAKE_TURN_FIXTURE: &str =
    include_str!("../../../tests/fixtures/protocol/fake-turn.jsonl");

/// One line of `tests/fixtures/protocol/fake-turn.jsonl`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FixtureEventLine {
    pub kind: RuntimeEventKind,
    #[serde(default)]
    pub delay_ms: u64,
    #[serde(default = "empty_object")]
    pub payload: Value,
}

fn empty_object() -> Value {
    Value::Object(serde_json::Map::new())
}

/// Parse a JSONL fixture. Blank lines and `//` comments are ignored.
pub fn parse_fixture_jsonl(text: &str) -> Result<Vec<FixtureEventLine>, RuntimeError> {
    let mut lines = Vec::new();
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let parsed: FixtureEventLine =
            serde_json::from_str(line).map_err(|e| RuntimeError::InvalidFixture {
                message: format!("line {}: {e}", idx + 1),
            })?;
        lines.push(parsed);
    }
    if lines.is_empty() {
        return Err(RuntimeError::InvalidFixture {
            message: "fixture contains no events".into(),
        });
    }
    Ok(lines)
}

pub fn load_fixture_file(path: impl AsRef<Path>) -> Result<Vec<FixtureEventLine>, RuntimeError> {
    let text = std::fs::read_to_string(path.as_ref()).map_err(|e| RuntimeError::InvalidFixture {
        message: format!("read {}: {e}", path.as_ref().display()),
    })?;
    parse_fixture_jsonl(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_committed_fixture_has_expected_shape() {
        let lines = parse_fixture_jsonl(FAKE_TURN_FIXTURE).unwrap();
        assert!(lines.len() >= 10);
        assert_eq!(lines.first().unwrap().kind, RuntimeEventKind::RunStarted);
        assert_eq!(lines.last().unwrap().kind, RuntimeEventKind::TurnCompleted);
        assert!(lines.iter().any(|l| l.kind == RuntimeEventKind::AssistantDelta));
        assert!(lines.iter().any(|l| l.kind == RuntimeEventKind::ToolCallStarted));
        assert!(lines.iter().any(|l| l.kind == RuntimeEventKind::SubagentStarted));
    }
}
