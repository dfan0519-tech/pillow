//! Typed JSON-RPC params builders (no raw secrets).

use serde_json::{Value, json};

/// The fixed built-ins the plaintext profile permits.
///
/// Upstream treats an empty `tools` array as "inherit every tool", so there is
/// no way to spell "none" — the list has to name something. `ask_user_question`
/// is inert. Native `write` is admitted only together with both ACP filesystem
/// capabilities, which makes the provider call Pillow's governed client RPC
/// instead of selecting its direct-to-disk LocalFs backend. Native
/// `run_terminal_cmd` is admitted only together with the ACP terminal
/// capability, preventing the same silent fallback to LocalTerminal.
const PLAINTEXT_ALLOWED_TOOLS: [&str; 3] = ["ask_user_question", "write", "run_terminal_cmd"];

/// Fixed session profile: the agent gets only native surfaces whose backends
/// are bounded by Pillow.
///
/// This is an **allowlist**, deliberately. A denylist is only an enumeration —
/// a live probe watched the agent, blocked from `write`, reach the same result
/// through `workflow` and `monitor`, which no denylist had thought to name.
/// Every tool the provider adds is a hole a denylist cannot know about, while
/// an allowlist fails closed.
///
/// `name` and `description` are **required**: upstream deserializes this object
/// with no defaults for them, and a parse failure is not an error the client
/// ever sees — the agent logs it and silently falls back to its *full* default
/// toolset. Pillow shipped exactly that shape and believed native tools were
/// off for weeks. Do not remove either field.
///
/// The two meta-tools through which every MCP tool is reached.
///
/// The allowlist cannot withhold these: upstream keeps `search_tool` and
/// `use_tool` alive no matter what the allowlist names, so that MCP stays
/// reachable. Its own test says so by name.
///
/// That matters because `mcpServers: []` does not mean "no MCP". The agent
/// merges servers from the user's config, plugins, and an account gateway, and
/// there is no field that says "use only what the client supplied". A live
/// session under the allowlist reached the account's `tasks` MCP and really
/// created an automation. Denying the two meta-tools is the only lever the
/// client has, and denials run before the allowlist, so it is available.
const MCP_META_TOOLS: [&str; 2] = ["search_tool", "use_tool"];

fn plaintext_agent_profile() -> Value {
    json!({
        "name": "pillow-plaintext",
        "description": "Pillow-hosted session. Native authority is restricted; \
                        file writes and shell commands run through governed client RPC.",
        "tools": PLAINTEXT_ALLOWED_TOOLS,
        "disallowedTools": MCP_META_TOOLS
    })
}

pub fn initialize_params(client_name: &str, client_version: &str) -> Value {
    json!({
        "protocolVersion": 1,
        "clientCapabilities": {
            "fs": { "readTextFile": true, "writeTextFile": true },
            "terminal": true
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
    fn initialize_declares_governed_fs_and_governed_terminal() {
        let p = initialize_params("Pillow", "0.1.0");
        assert_eq!(p["clientCapabilities"]["terminal"], true);
        assert_eq!(p["clientCapabilities"]["fs"]["writeTextFile"], true);
        assert_eq!(p["clientCapabilities"]["fs"]["readTextFile"], true);
    }

    /// Native `write` plus a false ACP write capability silently selects the
    /// provider's LocalFs backend. The child then writes directly without ever
    /// contacting Pillow, so this implication must never be split across
    /// changes or weakened into two unrelated shape assertions.
    #[test]
    fn native_write_allowlist_requires_acp_write_capability_in_the_same_profile() {
        let params = session_new_params("C:/project");
        let tools = params["_meta"]["agentProfile"]["tools"]
            .as_array()
            .expect("profile tools must be an allowlist");
        let permits_native_write = tools.iter().any(|tool| tool == "write");
        let acp_write_enabled = initialize_params("Pillow", "0.1.0")["clientCapabilities"]["fs"]["writeTextFile"]
            == true;

        assert!(
            !permits_native_write || acp_write_enabled,
            "native write with writeTextFile=false silently bypasses Pillow through LocalFs"
        );
        assert!(
            permits_native_write,
            "the shipped profile must expose the native write hand that reaches ACP"
        );
        assert!(
            acp_write_enabled,
            "the ACP write backend must ship with that native write hand"
        );
    }

    /// A native shell hand plus a false ACP terminal capability silently
    /// selects the provider's LocalTerminal backend. The child then spawns the
    /// process directly without ever contacting Pillow, so this implication
    /// must never be split across changes or weakened into unrelated shape
    /// assertions: the unsafe half-open combination fails silently.
    #[test]
    fn native_shell_allowlist_requires_acp_terminal_capability_in_the_same_profile() {
        let params = session_new_params("C:/project");
        let tools = params["_meta"]["agentProfile"]["tools"]
            .as_array()
            .expect("profile tools must be an allowlist");
        let permits_native_shell = tools.iter().any(|tool| {
            matches!(
                tool.as_str(),
                Some("run_terminal_cmd" | "run_terminal_command" | "bash")
            )
        });
        let acp_terminal_enabled =
            initialize_params("Pillow", "0.1.0")["clientCapabilities"]["terminal"] == true;

        assert!(
            !permits_native_shell || acp_terminal_enabled,
            "native shell with terminal=false silently bypasses Pillow through LocalTerminal"
        );
        assert!(
            permits_native_shell,
            "the shipped profile must expose the native shell hand that reaches ACP"
        );
        assert!(
            acp_terminal_enabled,
            "the ACP terminal backend must ship with that native shell hand"
        );
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
                "name": "pillow-plaintext",
                "description": "Pillow-hosted session. Native authority is restricted; \
                                file writes and shell commands run through governed client RPC.",
                "tools": ["ask_user_question", "write", "run_terminal_cmd"],
                "disallowedTools": ["search_tool", "use_tool"]
            }),
            "the session profile must not grow implicit capabilities"
        );
    }

    /// The two fields upstream requires, asserted on their own.
    ///
    /// Kept separate from the exact-shape test above because this is the pair
    /// whose absence is silent: upstream logs the parse failure and falls back
    /// to its full default toolset, so a profile missing them looks like a
    /// working restriction and is the opposite of one.
    #[test]
    fn profile_carries_the_fields_upstream_requires_to_parse() {
        for params in [
            session_new_params("C:/project"),
            session_load_params("session-1", "C:/project"),
        ] {
            let profile = &params["_meta"]["agentProfile"];
            for field in ["name", "description"] {
                assert!(
                    profile[field]
                        .as_str()
                        .is_some_and(|v| !v.trim().is_empty()),
                    "agentProfile.{field} is required upstream; without it the \
                     profile is dropped and every native tool comes back"
                );
            }
        }
    }

    /// An empty `tools` array means "inherit all" upstream, not "none".
    #[test]
    fn profile_never_ships_an_empty_tools_allowlist() {
        let tools = session_new_params("C:/project")["_meta"]["agentProfile"]["tools"].clone();
        let tools = tools.as_array().expect("tools must be an array");
        assert!(
            !tools.is_empty(),
            "an empty allowlist is read upstream as inherit-all, which grants \
             every native tool — name what is permitted instead"
        );
    }

    /// The denylist has exactly one job, and native gating is not it.
    ///
    /// Denying native tools was tried and failed: blocked from `write`, the
    /// agent reached the same result through `workflow` and `monitor`, which
    /// no denylist had thought to name. Native gating belongs to the allowlist,
    /// which fails closed. The denylist survives only to cut the two MCP
    /// meta-tools, which the allowlist is structurally unable to withhold.
    /// Anything else appearing here means someone is enumerating again.
    #[test]
    fn denylist_covers_only_the_mcp_meta_tools() {
        let profile = &session_new_params("C:/project")["_meta"]["agentProfile"];
        let denied = profile["disallowedTools"]
            .as_array()
            .expect("the MCP meta-tools must be denied");
        let mut denied: Vec<&str> = denied.iter().filter_map(|v| v.as_str()).collect();
        denied.sort_unstable();
        let mut expected: Vec<&str> = MCP_META_TOOLS.to_vec();
        expected.sort_unstable();
        assert_eq!(
            denied, expected,
            "the denylist exists to cut MCP discovery, not to gate native tools \
             — an enumeration cannot bound what the provider adds next"
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

            // Deliberately not `tools == []`. That is what this assertion used
            // to require, and it encoded the defect: upstream reads an empty
            // allowlist as inherit-all, so the test was demanding the one shape
            // that grants everything. Name the permitted set instead.
            assert_eq!(
                params["_meta"]["agentProfile"]["tools"],
                json!(PLAINTEXT_ALLOWED_TOOLS),
                "the plaintext profile may expose only the inert prompt tool and governed ACP write/shell hands"
            );
            for native in [
                "write_file",
                "search_replace",
                "bash",
                "read_file",
                "workflow",
                "monitor",
            ] {
                assert!(
                    !params["_meta"]["agentProfile"]["tools"]
                        .as_array()
                        .is_some_and(|tools| tools.iter().any(|t| t == native)),
                    "the plaintext profile must never permit native {native}"
                );
            }
        }
    }
}
