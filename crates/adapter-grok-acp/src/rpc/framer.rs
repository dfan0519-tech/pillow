//! Newline-delimited JSON framing with hard byte limits.

use crate::error::{invalid_json, output_too_large, transport};
use keeper_agent_protocol::RuntimeError;
use serde_json::Value;

/// Incremental NDJSON line framer.
#[derive(Debug, Default)]
pub struct LineFramer {
    buffer: Vec<u8>,
    max_line_bytes: usize,
}

impl LineFramer {
    pub fn new(max_line_bytes: usize) -> Self {
        Self {
            buffer: Vec::new(),
            max_line_bytes: max_line_bytes.max(1),
        }
    }

    /// Push a read chunk. Returns complete lines (without trailing `\n`).
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Vec<u8>>, RuntimeError> {
        if self.buffer.len().saturating_add(chunk.len()) > self.max_line_bytes
            && !chunk.contains(&b'\n')
        {
            // Fast path: even with newlines we must never retain > max after a line.
            // Check cumulative buffer growth before scanning.
        }
        self.buffer.extend_from_slice(chunk);
        if self.buffer.len() > self.max_line_bytes && !self.buffer.contains(&b'\n') {
            self.buffer.clear();
            return Err(output_too_large());
        }

        let mut lines = Vec::new();
        while let Some(idx) = self.buffer.iter().position(|b| *b == b'\n') {
            let mut line = self.buffer.drain(..=idx).collect::<Vec<u8>>();
            line.pop(); // drop '\n'
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.len() > self.max_line_bytes {
                self.buffer.clear();
                return Err(output_too_large());
            }
            if !line.is_empty() {
                lines.push(line);
            }
            // After taking a line, remaining buffer may still be large if next line incomplete.
            if self.buffer.len() > self.max_line_bytes && !self.buffer.contains(&b'\n') {
                self.buffer.clear();
                return Err(output_too_large());
            }
        }
        if self.buffer.len() > self.max_line_bytes {
            self.buffer.clear();
            return Err(output_too_large());
        }
        Ok(lines)
    }

    pub fn parse_line(line: &[u8]) -> Result<Value, RuntimeError> {
        if line.iter().all(|b| b.is_ascii_whitespace()) {
            return Err(invalid_json());
        }
        serde_json::from_slice(line).map_err(|_| invalid_json())
    }

    pub fn encode_message(value: &Value) -> Result<String, RuntimeError> {
        serde_json::to_string(value).map_err(|e| transport(format!("serialize failed: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn splits_on_newlines() {
        let mut f = LineFramer::new(1024);
        let lines = f
            .push(
                br#"{"a":1}
{"b":2}
"#,
            )
            .unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(LineFramer::parse_line(&lines[0]).unwrap(), json!({"a": 1}));
        assert_eq!(LineFramer::parse_line(&lines[1]).unwrap(), json!({"b": 2}));
    }

    #[test]
    fn rejects_oversized_line() {
        let mut f = LineFramer::new(16);
        let big = vec![b'x'; 32];
        let err = f.push(&big).unwrap_err();
        assert!(err.to_string().contains("acp_output_too_large"));
    }

    #[test]
    fn rejects_malformed_json() {
        let err = LineFramer::parse_line(br"{not-json").unwrap_err();
        assert!(err.to_string().contains("acp_invalid_json"));
    }

    #[test]
    fn ignores_empty_lines_in_stream() {
        let mut f = LineFramer::new(1024);
        let lines = f.push(b"\n\n{\"ok\":true}\n\n").unwrap();
        assert_eq!(lines.len(), 1);
    }
}
