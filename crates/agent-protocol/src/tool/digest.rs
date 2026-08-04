use serde_json::Value;
use sha2::{Digest, Sha256};

use super::protocol_error::{ToolProtocolError, ToolProtocolErrorCode};
use super::types::{ArgumentsDigest, WRITE_CONTENT_BYTE_CAP};

// fs.write@1 content may be any non-NUL UTF-8 up to WRITE_CONTENT_BYTE_CAP.
// JSON string encoding of a control byte is six bytes (`\u00XX`); the path is
// already bounded at 4_096 bytes and the fixed schema envelope is tiny.
// Cap = sixfold content escape + max path + quotes/keys/mode/braces margin.
const MAX_ARGUMENT_BYTES: usize = (WRITE_CONTENT_BYTE_CAP as usize) * 6 + 4_096 + 256;
const MAX_ARGUMENT_DEPTH: usize = 32;

pub fn canonical_arguments_digest(arguments: &Value) -> Result<ArgumentsDigest, ToolProtocolError> {
    if !arguments.is_object() {
        return Err(invalid_arguments("arguments must be a JSON object"));
    }
    let mut canonical = String::new();
    write_canonical(arguments, 0, &mut canonical)?;
    if canonical.len() > MAX_ARGUMENT_BYTES {
        return Err(invalid_arguments(format!(
            "canonical arguments exceed the {MAX_ARGUMENT_BYTES}-byte protocol cap"
        )));
    }
    let digest = Sha256::digest(canonical.as_bytes());
    let hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(ArgumentsDigest::from_sha256_hex(hex))
}

fn write_canonical(
    value: &Value,
    depth: usize,
    output: &mut String,
) -> Result<(), ToolProtocolError> {
    if depth > MAX_ARGUMENT_DEPTH {
        return Err(invalid_arguments(format!(
            "arguments exceed the maximum nesting depth of {MAX_ARGUMENT_DEPTH}"
        )));
    }
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
        Value::Number(value) => {
            if value.as_i64().is_none() && value.as_u64().is_none() {
                return Err(invalid_arguments(
                    "arguments may contain only integer JSON numbers",
                ));
            }
            output.push_str(&value.to_string());
        }
        Value::String(value) => {
            output.push_str(
                &serde_json::to_string(value).map_err(|_| invalid_arguments("invalid string"))?,
            );
        }
        Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_canonical(value, depth + 1, output)?;
            }
            output.push(']');
        }
        Value::Object(values) => {
            output.push('{');
            let mut keys = values.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            for (index, key) in keys.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                output.push_str(
                    &serde_json::to_string(key).map_err(|_| invalid_arguments("invalid key"))?,
                );
                output.push(':');
                write_canonical(&values[key], depth + 1, output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

fn invalid_arguments(_message: impl Into<String>) -> ToolProtocolError {
    ToolProtocolError::new(ToolProtocolErrorCode::InvalidArguments)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sha256_matches_the_empty_object_vector() {
        assert_eq!(
            canonical_arguments_digest(&json!({})).unwrap().as_str(),
            "sha256:44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
        );
    }

    #[test]
    fn full_write_content_of_control_bytes_that_need_u00xx_escapes_is_within_cap() {
        // `\u{1}` serializes as `\u0001` (six JSON bytes per content byte).
        let content = "\u{1}".repeat(WRITE_CONTENT_BYTE_CAP as usize);
        assert_eq!(content.len(), WRITE_CONTENT_BYTE_CAP as usize);
        let escaped = serde_json::to_string(&content).unwrap();
        assert!(escaped.contains("\\u0001"));
        assert_eq!(escaped.len(), 2 + (WRITE_CONTENT_BYTE_CAP as usize) * 6);

        let arguments = json!({
            "path": "src/a.ts",
            "mode": "create",
            "content": content,
        });
        assert!(canonical_arguments_digest(&arguments).is_ok());
    }
}
