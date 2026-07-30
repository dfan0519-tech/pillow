//! Typed JSON-RPC params builders (no raw secrets).

use serde_json::{Value, json};

fn plaintext_agent_profile() -> Value {
    json!({
        "tools": [],
        "disallowedTools": [
            "read_file",
            "grep_search",
            "list_dir",
            "bash",
            "run_terminal_command",
            "search_replace",
            "write_file",
            "task",
            "kill_task",
            "get_task_output",
            "search_tool",
            "use_tool",
            "web_search",
            "web_fetch",
            "memory_search",
            "memory_get",
            "todo_write",
            "lsp"
        ]
    })
}

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
        "mcpServers": [],
        "_meta": {
            "agentProfile": plaintext_agent_profile()
        }
    })
}

pub fn session_load_params(session_id: &str, cwd: &str) -> Value {
    json!({
        "sessionId": session_id,
        "cwd": cwd,
        "mcpServers": [],
        "_meta": {
            "agentProfile": plaintext_agent_profile()
        }
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

    #[test]
    fn new_and_load_use_the_same_fixed_plaintext_agent_profile() {
        let new_params = session_new_params("C:/project");
        let load_params = session_load_params("session-1", "C:/project");
        let new_profile = &new_params["_meta"]["agentProfile"];
        let load_profile = &load_params["_meta"]["agentProfile"];

        assert_eq!(new_profile, load_profile);
        assert_eq!(
            new_profile,
            &json!({
                "tools": [],
                "disallowedTools": [
                    "read_file",
                    "grep_search",
                    "list_dir",
                    "bash",
                    "run_terminal_command",
                    "search_replace",
                    "write_file",
                    "task",
                    "kill_task",
                    "get_task_output",
                    "search_tool",
                    "use_tool",
                    "web_search",
                    "web_fetch",
                    "memory_search",
                    "memory_get",
                    "todo_write",
                    "lsp"
                ]
            }),
            "the session profile must not grow implicit capabilities"
        );
    }

    #[test]
    fn session_profile_enables_no_approval_or_dangerous_capabilities() {
        for params in [
            session_new_params("C:/project"),
            session_load_params("session-1", "C:/project"),
        ] {
            assert_eq!(
                params["mcpServers"],
                json!([]),
                "ACP requires the field, but no MCP server may be attached"
            );
            let encoded = params.to_string().to_ascii_lowercase();
            for forbidden in ["yolomode", "automode", "always-approve"] {
                assert!(
                    !encoded.contains(forbidden),
                    "session params contain forbidden approval mode {forbidden}: {encoded}"
                );
            }

            assert_eq!(
                params["_meta"]["agentProfile"]["tools"],
                json!([]),
                "the plaintext profile may not expose provider tools"
            );
        }
    }
}
