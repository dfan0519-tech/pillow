# Grok Build ACP Protocol Research (0.2.112)

Date: 2026-07-30  
Status: Live probe complete  
Scope: Read-only product research for Keeper Slice B2 Rust ACP adapter  
CLI binary: `%USERPROFILE%\.grok\bin\grok.exe`  
Host worktree: this Pillow worktree (paths redacted as `<worktree>`)

## 1. Summary

Installed **Grok Build `0.2.112` (`9bbd559437`) [stable]** speaks a newline-delimited JSON-RPC 2.0 ACP over stdio when launched with:

```text
grok agent --no-leader stdio
```

Observed core path (sufficient for a first adapter):

1. `initialize` → protocolVersion `1`, `agentCapabilities.loadSession: true`
2. `session/new` → UUID `sessionId` plus model/`_meta` envelope
3. `session/prompt` → streaming `session/update` notifications, then result `{ stopReason }`
4. `session/cancel` → prompt result `stopReason: "cancelled"`
5. `session/load` → succeeds; replays history as `session/update` chunks
6. `session/request_permission` (server→client request) for non-auto-approved tools

**Recommendation: PASS** for implementing the Slice B2 Rust ACP adapter against this wire protocol. Residual HOLDs are listed in §14 (vendor extensions, unobserved stop reasons, permission-mode matrix).

Auth files were not inspected. No write tool was allowed to execute. A deliberate write-permission probe rejected the tool; the target file was confirmed absent afterward.

---

## 2. CLI version and launch

| Item | Value |
| --- | --- |
| Version command | `grok --version` / `grok version` |
| Version string | `grok 0.2.112 (9bbd559437) [stable]` |
| Exact launch | `grok agent --no-leader stdio` |
| Working directory | Child inherits host cwd; `session/new.params.cwd` sets session root |
| Leader mode | `--no-leader` forces a private agent process (required when config enables leader) |
| Stdio help surface | `--debug`, `--debug-file`, `--leader-socket`, `-h` only |

Related top-level flags (interactive `grok`, not exposed on `agent stdio` help) that still matter for host policy design:

- `--permission-mode` (`default`, `acceptEdits`, `auto`, `dontAsk`, `bypassPermissions`, `plan`)
- `--always-approve`
- `--model`, `--reasoning-effort`

These were **not** passed on the stdio launch used here; default agent permission behavior was observed instead.

---

## 3. Framing, transport, process lifecycle

### 3.1 Framing

- **Newline-delimited JSON (NDJSON)** on stdin/stdout.
- One JSON-RPC object per line, terminated by `\n`.
- **Not** LSP-style `Content-Length` framing.
- Empty lines are ignorable on the client side (legacy client skips them).

### 3.2 Stderr

Stderr is free-form log text (ANSI-colored in default build). Observed classes:

| Level | Example (sanitized) | When |
| --- | --- | --- |
| ERROR | `failed to decode Some(RawValue({})): Method not found` | Unsupported client notification (`authenticated`) |
| ERROR | `failed to decode Some(RawValue({"requestId":4})): Method not found` | Unsupported `$/cancel_request` |
| ERROR | `failed to parse incoming message: key must be a string … Raw: {not-json` | Malformed client JSON |
| WARN | `Old session thread still running after 5s — proceeding with replay…` | `session/load` while prior turn actor still settling |

Stderr is diagnostic only; protocol traffic is stdout.

### 3.3 Exit

- Graceful stdin close + kill: process exited with signal `SIGTERM` when the probe terminated the child after probes.
- Agent stayed alive through unknown methods and malformed input (see §11).
- No durable domain state was written by this research harness into Keeper product code.

### 3.4 Timeout

- Agent did not impose a client-visible short timeout on `session/prompt` in these runs (prompts finished in ~3–12s).
- Legacy client default is **3 minutes** per request (`DEFAULT_REQUEST_TIMEOUT_MS`).
- Adapter should keep an application-level request timeout independent of the agent.

---

## 4. `initialize`

### 4.1 Request (client → agent)

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": 1,
    "clientCapabilities": {
      "fs": { "readTextFile": false, "writeTextFile": false },
      "terminal": false
    },
    "clientInfo": {
      "name": "Keeper Protocol Probe",
      "version": "0.0.0-research"
    }
  }
}
```

Legacy Vault Keeper uses the same shape with `clientInfo.name = "Vault Keeper"`.

### 4.2 Result (agent → client) — observed capabilities

Sanitized essential fields:

```json
{
  "protocolVersion": 1,
  "agentCapabilities": {
    "loadSession": true,
    "promptCapabilities": {
      "image": false,
      "audio": false,
      "embeddedContext": true
    },
    "mcpCapabilities": {
      "http": true,
      "sse": true
    },
    "sessionCapabilities": {},
    "auth": {},
    "_meta": {
      "x.ai/fs_notify": true,
      "x.ai/hooks": {
        "blockingEvents": ["pre_tool_use", "stop", "subagent_stop"],
        "decisions": ["deny", "block"],
        "stopSignals": ["continue", "stopReason", "additionalContext"]
      },
      "x.ai/capabilities": {
        "toolOverrides": {
          "x_keyword_search": true,
          "x_semantic_search": true,
          "x_user_search": false,
          "x_thread_fetch": false
        }
      }
    }
  },
  "authMethods": [
    {
      "id": "cached_token",
      "name": "cached_token",
      "description": "Cached token from ~/.grok/auth.json"
    },
    {
      "id": "grok.com",
      "name": "Grok",
      "description": "Sign in with Grok"
    }
  ],
  "_meta": {
    "grokShell": true,
    "defaultAuthMethodId": "cached_token",
    "agentVersion": "0.2.112",
    "agentId": "<uuid>",
    "agentInstanceId": "<uuid>",
    "hostname": "<host>",
    "currentWorkingDirectory": "<worktree>",
    "modelState": {
      "currentModelId": "grok-4.5",
      "availableModels": [
        {
          "modelId": "grok-4.5",
          "name": "Grok 4.5",
          "description": "SpaceXAI's new frontier model",
          "_meta": {
            "agentType": "grok-build-plan",
            "supportsReasoningEffort": true,
            "reasoningEffort": "high",
            "reasoningEfforts": [
              { "id": "high", "value": "high", "label": "High Effort", "default": true },
              { "id": "medium", "value": "medium", "label": "Medium Effort", "default": false },
              { "id": "low", "value": "low", "label": "Low Effort", "default": false }
            ]
          }
        }
      ]
    },
    "availableCommands": [
      { "name": "compact", "description": "Compress conversation history to save context window" },
      { "name": "always-approve", "description": "Toggle always-approve mode (skip all permission prompts)" },
      { "name": "context", "description": "Show context window usage and session stats" },
      { "name": "session-info", "description": "Show session details (model, turns, context usage)" },
      { "name": "deep-research", "description": "Research with bounded parallel agents…" },
      { "name": "workflow", "description": "Launch a saved workflow…" },
      { "name": "goal", "description": "Set, manage, or check an autonomous goal" }
    ],
    "cancelRewind": true,
    "sessionRecap": true,
    "voiceMode": true
  }
}
```

### 4.3 Capability interpretation for the adapter

| Flag | Observed | Adapter meaning |
| --- | --- | --- |
| `protocolVersion` | `1` | Negotiate v1 only for now |
| `agentCapabilities.loadSession` | `true` | Resume via `session/load` |
| `sessionCapabilities.resume` | absent (`{}`) | Do **not** call `session/resume` |
| `sessionCapabilities.close` | absent | Do **not** call `session/close` |
| `promptCapabilities.embeddedContext` | `true` | Context embedding supported (not exercised) |
| `authMethods` / `defaultAuthMethodId` | `cached_token` | Host must not re-implement auth in B2; rely on installed CLI auth |

Token counts and cost fields appear under `_meta` in later results; treat as opaque and redact in diagnostics.

### 4.4 Post-initialize noise

Immediately after `initialize` / `session/new`, the agent emits many **vendor notifications** (`_x.ai/*`). See §8. They are optional for B2 core streaming.

Unsupported client notification experiment:

```json
{ "jsonrpc": "2.0", "method": "authenticated", "params": {} }
```

→ stderr `Method not found`; process remains alive. **Do not send `authenticated`.**

---

## 5. Session new / load / resume / close

### 5.1 `session/new`

Request:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "session/new",
  "params": {
    "cwd": "<worktree>",
    "mcpServers": []
  }
}
```

Legacy also sends optional `_meta.rules` (string). Not required for open.

Result (sanitized):

```json
{
  "sessionId": "019fb1af-4d95-7af1-bbc4-1ed5ddd7e9ea",
  "models": {
    "currentModelId": "grok-4.5",
    "availableModels": [ { "modelId": "grok-4.5", "name": "Grok 4.5" } ]
  },
  "_meta": {
    "currentWorkingDirectory": "<worktree>",
    "isGitRepo": true,
    "gitRoot": "<worktree>",
    "x.ai/sessionDetail": {
      "sessionId": "019fb1af-4d95-7af1-bbc4-1ed5ddd7e9ea",
      "kind": "build",
      "cwd": "<worktree>",
      "currentModelId": "grok-4.5"
    }
  }
}
```

`sessionId` is a UUID string (here UUID-like with version nibble `7` — treat as opaque string, not domain `Uuid` without parsing checks).

### 5.2 `session/load`

Request:

```json
{
  "jsonrpc": "2.0",
  "id": 5,
  "method": "session/load",
  "params": {
    "sessionId": "<prior-session-id>",
    "cwd": "<worktree>",
    "mcpServers": []
  }
}
```

Result: non-null object with `models` + `_meta` (includes session title once history exists). **Not** `null` (legacy test fixture used `null`; live agent returns a structured result).

Side effects observed:

- Replays prior turn as `session/update` notifications:
  - `user_message_chunk`
  - `agent_thought_chunk`
  - `agent_message_chunk`
  - `available_commands_update`
- Vendor `_x.ai/session/update` with `"_meta": { "isReplay": true }` for hooks
- If prior turn actor still running: stderr WARN and partial replay risk

**Adapter rule:** ignore replay updates unless a prompt is active (legacy already does this via `activePrompt` session match). Map load → host “resume session”, not a new user turn.

### 5.3 `session/resume`

```json
{ "method": "session/resume", "params": { "sessionId": "…", "cwd": "…", "mcpServers": [] } }
```

→ JSON-RPC error:

```json
{ "code": -32601, "message": "Method not found" }
```

Matches empty `sessionCapabilities`.

### 5.4 `session/close`

→ `-32601 Method not found`.

Lifecycle end is process kill / stdin close, not a session close RPC.

---

## 6. Prompt: request, streaming, completion

### 6.1 Request

```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "session/prompt",
  "params": {
    "sessionId": "<session-id>",
    "prompt": [
      {
        "type": "text",
        "text": "Reply with exactly this single line and nothing else: protocol-probe-ok. Do not use tools. …"
      }
    ]
  }
}
```

`prompt` is an **array of content blocks**, not a bare string.

### 6.2 Result

Successful no-tool turn:

```json
{
  "stopReason": "end_turn",
  "_meta": {
    "sessionId": "<session-id>",
    "requestId": "<uuid>",
    "promptId": "<uuid>",
    "modelId": "grok-4.5",
    "usage": {
      "modelCalls": 1,
      "apiDurationMs": 2807,
      "numTurns": 1
    }
  }
}
```

Token/cost fields exist under `_meta` / `_meta.usage` and should be redacted in logs.

### 6.3 Observed `stopReason` values

| stopReason | cancellationCategory (`_meta`) | Cause |
| --- | --- | --- |
| `end_turn` | (absent) | Normal completion |
| `cancelled` | `MidTurnAbort` | `session/cancel` during generation |
| `cancelled` | `PermissionRejected` | Client selected `reject_once` on permission |

Not observed in this research: `max_tokens`, `refusal`, tool-loop limits, auth failures. Adapter must treat unknown `stopReason` as non-success diagnostic, not crash.

### 6.4 Vendor completion signal

In addition to the JSON-RPC response, agent notifies:

```json
{
  "jsonrpc": "2.0",
  "method": "_x.ai/session/prompt_complete",
  "params": {
    "sessionId": "<session-id>",
    "promptId": "<uuid>",
    "stopReason": "end_turn",
    "agentResult": null
  }
}
```

Core adapter can ignore this if it keys completion off the `session/prompt` response; useful as a secondary signal if the response is delayed.

---

## 7. Standard `session/update` notification shapes

Envelope:

```json
{
  "jsonrpc": "2.0",
  "method": "session/update",
  "params": {
    "sessionId": "<session-id>",
    "update": { "sessionUpdate": "<kind>", "…" },
    "_meta": { "…optional event metadata…" }
  }
}
```

### 7.1 Kinds observed on the standard channel

| `sessionUpdate` | Role | Public to UI? |
| --- | --- | --- |
| `available_commands_update` | Slash-command catalog | Optional |
| `user_message_chunk` | Echo of user prompt (also on load replay) | Usually ignore for active turn assembly |
| `agent_thought_chunk` | Reasoning stream | Map to `reasoning_summary_delta` if product wants it; legacy **drops** it |
| `agent_message_chunk` | Assistant text stream | Map to `assistant_delta` |
| `tool_call` | Tool start | Map to `tool_call_started` |
| `tool_call_update` | Tool progress / terminal | Map to output / completed |

### 7.2 Text chunks

```json
{
  "sessionUpdate": "agent_message_chunk",
  "content": { "type": "text", "text": "protocol-probe-ok" }
}
```

```json
{
  "sessionUpdate": "agent_thought_chunk",
  "content": { "type": "text", "text": "The user wants me to…" }
}
```

```json
{
  "sessionUpdate": "user_message_chunk",
  "content": { "type": "text", "text": "<user prompt text>" },
  "_meta": { "modelId": "grok-4.5", "promptIndex": 0 }
}
```

Thought and message chunks may arrive as many tiny tokens or fewer larger pieces.

### 7.3 Tool call lifecycle (read — auto-approved)

**Start** (`tool_call`) — note: top-level `kind` often **missing** here:

```json
{
  "sessionUpdate": "tool_call",
  "toolCallId": "call-4b65e62b-5374-4409-927d-abc2633ba746-0",
  "title": "read_file",
  "rawInput": {
    "target_file": "<worktree>\\Agents.md",
    "limit": 20
  },
  "_meta": {
    "x.ai/tool": {
      "version": 1,
      "name": "read_file",
      "kind": "read",
      "namespace": "grok_build",
      "label": "Read",
      "read_only": true
    }
  }
}
```

**Enrichment** (`tool_call_update`) — `kind` appears:

```json
{
  "sessionUpdate": "tool_call_update",
  "toolCallId": "call-…-0",
  "kind": "read",
  "title": "Read `<worktree>\\Agents.md`",
  "locations": [{ "path": "<worktree>\\Agents.md" }],
  "rawInput": {
    "variant": "ReadFile",
    "target_file": "<worktree>\\Agents.md",
    "limit": 20
  },
  "_meta": {
    "x.ai/tool": {
      "name": "read_file",
      "kind": "read",
      "read_only": true,
      "input": { "path": "<worktree>\\Agents.md", "limit": 20 }
    }
  }
}
```

**Completion**:

```json
{
  "sessionUpdate": "tool_call_update",
  "toolCallId": "call-…-0",
  "status": "completed",
  "content": [
    {
      "type": "content",
      "content": {
        "type": "text",
        "text": "1→# Pillow Agent Rules\n…"
      }
    }
  ],
  "rawOutput": {
    "type": "ReadFile",
    "FileContent": {
      "absolute_path": "<worktree>\\AGENTS.md",
      "total_lines": 43
    }
  }
}
```

**Important for policy:**

- Prefer `update.kind` when present; fall back to `_meta["x.ai/tool"].kind`.
- Initial `tool_call` may only have `title` + `_meta.x.ai/tool`.
- Params `_meta.updateParams.kind` may say `"Other"` even when tool is a read — **do not trust** that field alone.
- `rawInput` / `rawOutput` can contain full file bodies — **never persist raw** into SQLite UI projections or logs.

### 7.4 Tool call lifecycle (write — permission required)

Start title `"write"`; enrichment `kind: "edit"` with diff content:

```json
{
  "sessionUpdate": "tool_call_update",
  "toolCallId": "call-…-0",
  "kind": "edit",
  "title": "Write `<worktree>\\docs\\research\\_MUST_NOT_BE_CREATED.txt`",
  "content": [
    {
      "type": "diff",
      "path": "<worktree>\\docs\\research\\_MUST_NOT_BE_CREATED.txt",
      "oldText": "",
      "newText": "DENIED\n"
    }
  ],
  "locations": [{ "path": "<worktree>\\docs\\research\\_MUST_NOT_BE_CREATED.txt" }],
  "_meta": {
    "x.ai/tool": {
      "name": "write",
      "kind": "write",
      "namespace": "opencode",
      "read_only": false
    }
  }
}
```

After reject:

```json
{
  "sessionUpdate": "tool_call_update",
  "toolCallId": "call-…-0",
  "status": "failed",
  "content": [
    {
      "type": "content",
      "content": {
        "type": "text",
        "text": "User rejected the execution for tool `write`"
      }
    }
  ]
}
```

Statuses observed: `completed`, `failed`. Legacy also expects `pending` / `in_progress` semantics; live `_meta.updateParams.status` uses PascalCase (`Pending`, `Completed`, `Failed`) while `update.status` uses lowercase (`completed`, `failed`).

### 7.5 `available_commands_update`

```json
{
  "sessionUpdate": "available_commands_update",
  "availableCommands": [
    {
      "name": "compact",
      "description": "Compress conversation history to save context window",
      "input": { "hint": "optional context about what to preserve" }
    }
  ]
}
```

Emitted at session start and after turns. Optional for B2.

---

## 8. Vendor `_x.ai/*` notifications (observed)

These are **not** required to complete a prompt, but they fire densely. Adapter should:

- parse JSON safely;
- ignore unknown methods by default;
- never fail the connection on vendor methods.

| Method | Purpose (observed) |
| --- | --- |
| `_x.ai/mcp/servers_updated` | MCP server list (`mcpServers: []`) |
| `_x.ai/mcp_initialized` | MCP ready for session |
| `_x.ai/announcements/update` | Product announcements |
| `_x.ai/settings/update` | UI tips / settings flags |
| `_x.ai/models/update` | Model list / current model |
| `_x.ai/sessions/changed` | Session roster upsert/remove (`activity`: `working`, etc.) |
| `_x.ai/queue/changed` | Prompt queue entries for session |
| `_x.ai/session_notification` | Extended session updates (hooks, tool deltas, interactions) |
| `_x.ai/session/update` | Parallel session updates (often replay-tagged) |
| `_x.ai/session/prompt_complete` | Duplicate completion signal |

### 8.1 Permission-related vendor updates

```json
{
  "method": "_x.ai/session_notification",
  "params": {
    "sessionId": "<id>",
    "update": {
      "sessionUpdate": "pending_interaction",
      "tool_call_id": "call-…",
      "kind": "permission"
    }
  }
}
```

```json
{
  "method": "_x.ai/session_notification",
  "params": {
    "sessionId": "<id>",
    "update": {
      "sessionUpdate": "interaction_resolved",
      "tool_call_id": "call-…"
    }
  }
}
```

```json
{
  "method": "_x.ai/session_notification",
  "params": {
    "sessionId": "<id>",
    "update": {
      "sessionUpdate": "tool_call_delta_chunk",
      "tool_call_id": "call-…",
      "tool_index": 0,
      "name": "write"
    }
  }
}
```

Hook execution sample:

```json
{
  "method": "_x.ai/session_notification",
  "params": {
    "sessionId": "<id>",
    "update": {
      "sessionUpdate": "hook_execution",
      "event_name": "session_start",
      "runs": [
        {
          "name": "global/keeper-harness:session_start[0].hooks[0]",
          "status": { "status": "success", "elapsed_ms": 424 }
        }
      ]
    }
  }
}
```

---

## 9. Permission request shape

### 9.1 When it appears

| Tool class | Default agent behavior (this probe) |
| --- | --- |
| `read` / `read_file` | **No** `session/request_permission`; tool runs immediately |
| `write` / edit | **Yes** — blocks until client answers |

Host policy must still treat auto-approved reads as auditable tool events even when no permission RPC is sent.

### 9.2 Server → client request

```json
{
  "jsonrpc": "2.0",
  "id": 0,
  "method": "session/request_permission",
  "params": {
    "sessionId": "<session-id>",
    "toolCall": {
      "toolCallId": "call-98dfe3bd-46c9-4d71-8d5f-1b3b66704b8c-0",
      "kind": "edit",
      "title": "Write `<worktree>\\docs\\research\\_MUST_NOT_BE_CREATED.txt`",
      "rawInput": {
        "variant": "Write",
        "file_path": "<worktree>\\docs\\research\\_MUST_NOT_BE_CREATED.txt",
        "content": "DENIED\n"
      },
      "_meta": {
        "x.ai/tool": {
          "version": 1,
          "name": "write",
          "kind": "write",
          "namespace": "opencode",
          "label": "Write",
          "read_only": false,
          "input": {
            "path": "<worktree>\\docs\\research\\_MUST_NOT_BE_CREATED.txt"
          }
        }
      }
    },
    "options": [
      {
        "optionId": "allow-edits-session",
        "name": "Yes, allow all edits during this session",
        "kind": "allow_always"
      },
      {
        "optionId": "allow-once",
        "name": "Yes",
        "kind": "allow_once"
      },
      {
        "optionId": "reject-once",
        "name": "No, and tell Grok what to do differently",
        "kind": "reject_once"
      }
    ]
  }
}
```

### 9.3 Client → server response

Legacy / observed successful reply shape:

```json
{
  "jsonrpc": "2.0",
  "id": 0,
  "result": {
    "outcome": {
      "outcome": "selected",
      "optionId": "reject-once"
    }
  }
}
```

Cancel without selecting an option (legacy late-cancel path):

```json
{
  "jsonrpc": "2.0",
  "id": 0,
  "result": {
    "outcome": { "outcome": "cancelled" }
  }
}
```

### 9.4 ID space warning

- Client requests used ids `1, 2, 3, …`
- Server permission request used id `0`

IDs are shared JSON-RPC id space on one connection. Adapter must:

- track pending client requests by id;
- answer server requests without treating them as client responses;
- avoid colliding with server ids (starting client ids at `1` is safe for this build; a split high-bit space is safer long-term).

### 9.5 Effect of `reject_once`

1. `tool_call_update` → `status: "failed"` with rejection text  
2. `session/prompt` result → `stopReason: "cancelled"`, `_meta.cancellationCategory: "PermissionRejected"`  
3. File **not** written  

Legacy maps reject to policy denial and may set `proposalRequired` for write kinds — still valid, but note that **reject ends the turn** in 0.2.112 rather than letting the model continue after denial in this probe.

---

## 10. Cancellation behavior

### 10.1 Working path

Notification (no response expected):

```json
{
  "jsonrpc": "2.0",
  "method": "session/cancel",
  "params": { "sessionId": "<session-id>" }
}
```

Observed outcome for in-flight `session/prompt`:

```json
{
  "stopReason": "cancelled",
  "_meta": {
    "cancellationCategory": "MidTurnAbort",
    "sessionId": "<session-id>",
    "requestId": "<uuid>",
    "promptId": "<uuid>"
  }
}
```

### 10.2 Non-working path (legacy sends this too)

```json
{
  "jsonrpc": "2.0",
  "method": "$/cancel_request",
  "params": { "requestId": 4 }
}
```

→ stderr `Method not found`. Harmless but useless on 0.2.112. Adapter may omit it.

### 10.3 Process-tree cancel

Not fully stress-tested here beyond `session/cancel` + child kill after probes. B2 host should still kill the process tree on hard cancel / drop (Slice B2 acceptance), because:

- cancel is cooperative at the protocol layer;
- `session/load` warned when a prior actor was still running after 5s.

---

## 11. Malformed messages and unknown methods

| Input | Agent behavior |
| --- | --- |
| `{not-json` line | stderr parse ERROR; **connection stays up** |
| Unknown method `session/does_not_exist` | JSON-RPC error `-32601 Method not found`; still alive |
| Unsupported notification methods | stderr ERROR decode; still alive |

Adapter should:

- close on **agent-originated** malformed JSON (legacy behavior) if the stream can no longer be synchronized;
- not exit solely because the agent logged a client-side parse error.

Line size: legacy caps at 1 MiB per buffered stream; tool `rawOutput` can be large — stream-parse and redact.

---

## 12. End-to-end sequences (condensed)

### 12.1 Happy path (no tools)

```text
→ initialize
← initialize result (capabilities)
→ session/new { cwd, mcpServers: [] }
← session/new result { sessionId }
← session/update available_commands_update (+ many _x.ai/*)
→ session/prompt { sessionId, prompt:[{type:text,text}] }
← session/update user_message_chunk
← session/update agent_thought_chunk*   (optional for UI)
← session/update agent_message_chunk*   (assistant text)
← _x.ai/session/prompt_complete
← { id, result: { stopReason: "end_turn", _meta… } }
```

### 12.2 Read tool (auto-approved)

```text
→ session/prompt
← tool_call (title/rawInput/_meta.x.ai/tool; kind may be absent)
← tool_call_update (kind=read, locations, rawInput)
← tool_call_update (status=completed, content, rawOutput)
← agent_message_chunk*
← result stopReason=end_turn
```

No `session/request_permission` observed for read.

### 12.3 Write tool (rejected)

```text
→ session/prompt
← tool_call (write)
← tool_call_update (kind=edit, diff content)
← _x.ai/… pending_interaction kind=permission
← session/request_permission { id:0, toolCall, options }
→ { id:0, result: { outcome: { outcome:"selected", optionId:"reject-once" } } }
← tool_call_update status=failed
← result stopReason=cancelled, cancellationCategory=PermissionRejected
```

### 12.4 Mid-turn cancel

```text
→ session/prompt (long generation)
→ session/cancel { sessionId }
← result stopReason=cancelled, cancellationCategory=MidTurnAbort
```

### 12.5 Load / resume

```text
→ session/load { sessionId, cwd, mcpServers: [] }
← models/_meta result
← replay session/update chunks (do not treat as new live turn)
```

`session/resume` / `session/close` → Method not found.

---

## 13. Gaps vs legacy `grok-acp-client.js` / tool policy

| Area | Legacy client | Live 0.2.112 | Adapter guidance |
| --- | --- | --- | --- |
| Framing | NDJSON | NDJSON | Match |
| `initialize` | protocolVersion 1, minimal client caps | Rich capabilities + `_meta` | Parse known fields; ignore extras |
| `loadSession` | top-level capability | `true` | Support `session/load` |
| `sessionCapabilities.resume/close` | optional | empty / unsupported | Do not call |
| `session/load` result | fixture used `null` | structured object | Accept object or null |
| Assistant text | only `agent_message_chunk` | same + thoughts | Drop or map thoughts separately |
| Tools | `tool_call` / `tool_call_update` | same + rich `_meta` | Prefer `kind` + `_meta.x.ai/tool` |
| Permission | always expected for tools in tests | **reads skip permission** | Policy must observe tool updates even without RPC |
| Permission options | `allow_once` / `reject_once` kinds | also `allow_always` (`allow-edits-session`) | Match on `kind`, not only optionId strings |
| Cancel | `session/cancel` + `$/cancel_request` | only `session/cancel` works | Prefer session cancel alone |
| `authenticated` | not sent | unsupported if sent | Do not send |
| Response limits | 12k chars / 24 tool events | not enforced by agent | Keep host-side limits |
| Rules | `_meta.rules` on `session/new` | not re-probed | Preserve as optional |
| Vendor `_x.ai/*` | ignored | noisy | Ignore by default |
| Permission reject | may continue after deny in tests | turn **cancelled** (`PermissionRejected`) | Treat as terminal cancelled run |

Legacy policy kinds (`read`, `edit`, `delete`, `move`, `search`, `execute`, `think`, `fetch`, `other`) still map, with live aliases:

- write tools often surface `kind: "edit"` on ACP fields and `kind: "write"` under `_meta.x.ai/tool`
- read tools use `kind: "read"` and `name: "read_file"`

---

## 14. Mapping sketch → Keeper neutral events

For Slice B2 adapter authors (not an implementation):

| Wire | Neutral `RuntimeEventKind` |
| --- | --- |
| start of `session/prompt` | `run_started` |
| `agent_message_chunk` | `assistant_delta` |
| `agent_thought_chunk` | `reasoning_summary_delta` (optional product flag) |
| `tool_call` | `tool_call_started` |
| `tool_call_update` with content/rawOutput | `tool_call_output` (redacted) |
| `tool_call_update` terminal status | `tool_call_completed` |
| `stopReason: end_turn` | `turn_completed` |
| RPC error / empty hard failure | `turn_failed` |
| `stopReason: cancelled` (any category) | `run_cancelled` |
| permission RPC | host approval domain (not a neutral stream event by itself) |

Do not leak `rawInput`, absolute secret paths, tokens, or full file bodies into SQLite event payloads without redaction policy.

---

## 15. Residual HOLDs (non-blocking for start)

1. Full **permission-mode** matrix (`default` vs `acceptEdits` vs `auto` vs `bypassPermissions`) not mapped via stdio flags.  
2. Unobserved stop reasons (`max_tokens`, auth expiry mid-prompt, network failure).  
3. Subagent / plan / workflow update shapes not forced in these probes (commands advertised only).  
4. Client filesystem capabilities (`readTextFile` / `writeTextFile` true) not exercised — agent may reverse-call the client.  
5. Exact process-tree cleanup on Windows under hung tools not measured.  
6. Multi-prompt concurrency / queue semantics only partially seen via `_x.ai/queue/changed`.

None of these block implementing the core adapter + fixture tests from this transcript.

---

## 16. PASS / HOLD recommendation

### **PASS — implement the Rust ACP adapter**

Rationale:

- Launch command and version pin are exact.
- Initialize, session new/load, prompt streaming, cancel, and permission RPC shapes are live-captured with sanitized examples.
- Framing, stderr, exit, and malformed behavior are known.
- Gaps versus the legacy Node client are explicit and adapter-actionable.
- Neutral event mapping is clear enough for Slice B2 without guessing.

### Conditions for implementation work

1. Pin probe version `0.2.112` in adapter diagnostics (`agentVersion` from initialize `_meta`).  
2. Fixture tests must cover: NDJSON round-trip, permission id handling, load replay ignore, cancel → `cancelled`, reject → `PermissionRejected`, tool kind late-bind.  
3. Never auto-allow write/execute/network tools without host approval policy.  
4. Redact `_meta` usage/cost/token fields and tool raw payloads in logs.  
5. Re-probe after any Grok Build minor upgrade before calling B2 complete.

---

## 17. Method checklist (this build)

| Method | Direction | Supported |
| --- | --- | --- |
| `initialize` | C→A request | Yes |
| `session/new` | C→A request | Yes |
| `session/load` | C→A request | Yes |
| `session/prompt` | C→A request | Yes |
| `session/cancel` | C→A notify | Yes |
| `session/request_permission` | A→C request | Yes (writes) |
| `session/resume` | C→A | No (`-32601`) |
| `session/close` | C→A | No (`-32601`) |
| `$/cancel_request` | C→A | No |
| `authenticated` | C→A | No |
| `session/update` | A→C notify | Yes |
| `_x.ai/*` | A→C notify | Yes (ignore-unknown) |

---

## 18. Research method notes

- Isolated processes: `grok agent --no-leader stdio` only.  
- Three live sessions: no-tool prompt + cancel + load; read-only tool; write permission reject.  
- No auth files read; secrets redacted if present in free text.  
- Temporary probe scripts and raw transcripts were used under `docs/research/` during capture and are **not** part of the product deliverable (this document only).  
- Reference clients: `legacy Keeper (out of tree)\lib\grok-acp-client.js`, `keeper-acp-tool-policy.js` (read-only).

---

## 19. Appendix — minimal client algorithm

```text
spawn(grok, ["agent", "--no-leader", "stdio"], cwd=projectRoot)
on stdout lines: JSON.parse →
  if method == session/request_permission: decide → reply result.outcome
  if method == session/update: normalize → EventSink
  if method starts with _x.ai/: ignore or soft-handle
  if id matches pending client request: resolve/reject
write line: JSON.stringify(msg) + "\n"
initialize → session/new|session/load → session/prompt
cancel: notify session/cancel; await prompt result or timeout then kill tree
```

This matches live 0.2.112 behavior observed on 2026-07-30.
