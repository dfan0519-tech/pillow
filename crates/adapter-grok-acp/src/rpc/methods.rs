//! Typed JSON-RPC params builders (no raw secrets).

use serde_json::{Value, json};

pub fn initialize_params(client_name: &str, client_version: &str) -> Value {
    json!({
        "protocolVersion": 1,
        "clientCapabilities": {
            "fs": { "readTextFile": false, "writeTextFile": false },
            "terminal": false
        },
        "clientInfo": {
            "name": client_name,
            "version": client_version
        }
    })
}

pub fn session_new_params(cwd: &str) -> Value {
    json!({
        "cwd": cwd,
        "mcpServers": []
    })
}

pub fn session_load_params(session_id: &str, cwd: &str) -> Value {
    json!({
        "sessionId": session_id,
        "cwd": cwd,
        "mcpServers": []
    })
}

pub fn session_prompt_params(session_id: &str, text: &str) -> Value {
    json!({
        "sessionId": session_id,
        "prompt": [{ "type": "text", "text": text }]
    })
}

pub fn session_cancel_params(session_id: &str) -> Value {
    json!({ "sessionId": session_id })
}

pub fn permission_result_selected(option_id: &str) -> Value {
    json!({
        "outcome": {
            "outcome": "selected",
            "optionId": option_id
        }
    })
}

pub fn permission_result_cancelled() -> Value {
    json!({
        "outcome": { "outcome": "cancelled" }
    })
}

pub fn permission_result(selected_option_id: Option<&str>) -> Value {
    match selected_option_id {
        Some(id) => permission_result_selected(id),
        None => permission_result_cancelled(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_declares_no_fs_or_terminal() {
        let p = initialize_params("Keeper IDE", "0.1.0");
        assert_eq!(p["clientCapabilities"]["terminal"], false);
        assert_eq!(p["clientCapabilities"]["fs"]["writeTextFile"], false);
        assert_eq!(p["clientCapabilities"]["fs"]["readTextFile"], false);
    }

    #[test]
    fn prompt_is_content_block_array() {
        let p = session_prompt_params("s1", "hello");
        assert_eq!(p["prompt"][0]["type"], "text");
        assert_eq!(p["prompt"][0]["text"], "hello");
    }
}
