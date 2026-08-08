# Upstream grok-build ACP baseline audit

**Date:** 2026-08-08  
**Worker:** grok-build-audit (Grok)  
**Nature:** Read-only investigation. No baseline pin update. No product code changes.  
**Working copy of upstream:** `%TEMP%\grokbuild-audit\grok-build` (deleted after audit)  
**Pillow baseline pin:** `docs/references/upstream-baseline.md` → `5da6962e4adb9c857f3def762542b52b4ec3e522` (2026-07-29 inspect, Apache-2.0, conceptual reference)

**Completion signal:** `UPSTREAM-GROKBUILD-AUDIT-COMPLETE`

---

## 0. Scope and hard limits (what this audit did not do)

| Done | Not done (by task contract) |
| --- | --- |
| Cloned public `https://github.com/xai-org/grok-build` to temp dir | No `cargo build` / `npm install` / upstream script execution |
| Static `git log` / `git diff` / source reading | No live ACP session (`grok agent … stdio` interactive) |
| Read Pillow adapter, fixtures, baseline doc | No product code edits except this report |
| `grok.exe --version`, `grok.exe agent --help` | No auth/token file reads; no desktop E2E; no `git add/commit` |
| GitHub API: commits / releases / tags | No burn of model quota for protocol probing |

Keeper Bridge boot (`POST http://127.0.0.1:7890/api/agent-bridge/boot`) returned **HTTP 400** in this environment; audit proceeded without a recorded bridge session.

---

## 1. Q1 — Upstream HEAD, tags, commits since baseline

### 1.1 Current HEAD

| Field | Value |
| --- | --- |
| Remote | `https://github.com/xai-org/grok-build` (`origin/main`) |
| HEAD | `afbc0fb710320c7add294c2106d447ecc3e3af2e` |
| Date | 2026-08-07 16:56:10 +0000 |
| Message | `Synced from monorepo` |
| Root `SOURCE_REV` (monorepo SHA recorded in tree) | `3e620a76a5f374ce644dc7c87f7e990c68348218` |
| License | Apache-2.0 (`LICENSE`) |

Evidence: local clone `git rev-parse HEAD`, `git log -1`, `SOURCE_REV`, `README.md` (states periodic monorepo sync).

### 1.2 Release tags

| Check | Result |
| --- | --- |
| `git tag -l` on clone | **Empty** (no tags) |
| `git ls-remote --tags origin` | **Empty** |
| GitHub API `/releases` | Empty array |
| GitHub API `/tags` | Empty |

**Conclusion:** There is **no published release tag** on the public mirror. Versioning for the shipping CLI is carried by `crates/codegen/xai-grok-version` / `CHANGELOG.md` / monorepo packaging, not by GitHub tags on `xai-org/grok-build`.

### 1.3 Commits from baseline `5da6962e` → HEAD

**Count:** **9** monorepo-sync commits (`git rev-list --count 5da6962e..HEAD`).

| Sync commit | Date (UTC) | `xai-grok-version` crate | `SOURCE_REV` (monorepo) |
| --- | --- | --- | --- |
| `5da6962e` (**Pillow baseline**) | 2026-07-28 | **0.2.112** | `2a818575225183d8ca915f5632a09b8067b5156a` |
| `500129c7` | 2026-07-29 | 0.2.114 | `6372e41d828b8a6ee82c29e01a69e27ec895cca9` |
| `dd04f397` | 2026-07-30 | 0.2.116 | `2a28b4a86cfc4a4c133c35b7fc2a6a9964387c39` |
| `a4221165` | 2026-07-31 | 0.2.117 | `8d69c91f02bcacf01e98d5aebbf2f92547c45738` |
| `780d1388` | 2026-08-03 | **0.2.118** | `64c4de99cc822b25ce9c54ab5a4f372093d0885d` |
| `e5478eff` | 2026-08-03 | 0.2.119 | `27d2088ae3b3f25e9ddab462caa18a07005ada9a` |
| `ed6d5436` | 2026-08-04 | 0.2.120 | `d6937fe255dce4133c3d000a50f9cb94de12f06f` |
| `a5589e95` | 2026-08-05 | 0.2.120 | `4d6d11372ab8f73026a78c45a7b7e7b1310eb39f` |
| `393430ee` | 2026-08-06 | 0.2.121 | `796754a8bf947b7c6c579049f94c7cfd0ac0ec03` |
| `afbc0fb7` (**HEAD**) | 2026-08-07 | **1.0.0** | `3e620a76a5f374ce644dc7c87f7e990c68348218` |

**Version span (crate string in tree):** `0.2.112` → `0.2.121` → packaging bump `1.0.0` on HEAD.  
**CHANGELOG** (`crates/codegen/xai-grok-shell/CHANGELOG.md`) documents user-facing `0.2.113`…`1.0.0` notes aligned with those bumps.

**Overall tree churn:** ~1017 files changed, large insertions (shell/pager/workspace). ACP **wire library** churn is small relative to product surface (see §2).

---

## 2. Q2 — ACP protocol surface changes (`5da6962e` → HEAD)

### 2.0 Protocol dependency lock (strongest wire-contract signal)

| Crate | Baseline `5da6962e` | HEAD `afbc0fb` |
| --- | --- | --- |
| `agent-client-protocol` | **0.10.4** checksum `10eeef5e…` | **0.10.4** same checksum |
| `agent-client-protocol-schema` | **0.11.4** checksum `ca68e7e5…` | **0.11.4** same checksum |
| `crates/codegen/xai-acp-lib/**` | present | **zero file changes** (`git diff --name-only` empty) |

Evidence: `Cargo.toml` workspace dep; `Cargo.lock` entries; empty name-status for `xai-acp-lib`.

**Interpretation:** The public ACP JSON-RPC method/type schema consumed by Grok did **not** bump between baseline and HEAD. Changes below are **agent implementation / capability advertising / product behavior**, not a new ACP crate revision.

---

### 2.1 `initialize` / session lifecycle (`new` / `load` / `resume`)

#### `initialize`

| Item | Evidence | Change vs baseline |
| --- | --- | --- |
| Method | `acp_agent.rs` `async fn initialize` (HEAD ~L42; baseline same method) | Still present |
| Protocol version | `InitializeResponse::new(acp::ProtocolVersion::V1)` HEAD ~L464; baseline same pattern | **Unchanged** |
| `agentCapabilities.loadSession` | `.load_session(true)` HEAD ~L466–467; baseline same | **Unchanged** |
| `_meta.agentVersion` | HEAD ~L508; baseline same key | **Unchanged** (Pillow reads this) |
| Auth advertising | Silent refresh path refactored (`SilentRefresh`, first-party env probe) | **Internal**; may change which auth methods are listed, not method name |
| Session capability ads | HEAD builds `SessionCapabilities` with `.close(...)` and optionally `.list` / `.resume` when chat modes allow (~L454–462) | **Additive** advertising of list/resume/close |

Pillow touchpoints:

- `crates/adapter-grok-acp/src/rpc/methods.rs` → `initialize_params` (`protocolVersion: 1`, fs read-only, no terminal)
- `crates/adapter-grok-acp/src/state.rs` → reads `/agentCapabilities/loadSession`, sets transport ready
- `crates/adapter-grok-acp/src/health.rs` → `/_meta/agentVersion`
- Fixture: `tests/fixtures/protocol/grok-acp/initialize-ok.jsonl`

#### `session/new` / `session/load`

| Item | Evidence | Change |
| --- | --- | --- |
| Trait methods | HEAD `new_session` / `load_session` still implement `acp::Agent` (~L930–940); body moved to `session_setup.rs` (`new_session_inner` / `load_session_inner`) starting at sync **`ed6d543`** | **Refactor** of file layout, not method removal |
| Request shape | Still `acp::NewSessionRequest` / `LoadSessionRequest` with `cwd` + `mcp_servers` (`session_setup.rs` ~L222–243, ~L672+) | **Compatible** with Pillow `session_new_params` / `session_load_params` |
| `_meta.agentProfile` | Still parsed (`agent_ops.rs` / `upload/turn.rs` comments + parse path); CHANGELOG/tests still treat it as client-supplied profile | **Still honored** |
| Capability note | initialize `_meta` still documents `_meta.pluginDirs` for session new/load (HEAD ~L499–501) | Additive; Pillow does not send pluginDirs |

Pillow: `adapter.rs` `session/new` + `session/load`; fixtures `session-new-prompt-text-only.jsonl`.

#### `session/resume` / `session/list` / `session/close` (**new agent-side implementations**)

| Method | First appears in interval | Notes |
| --- | --- | --- |
| `list_sessions` | sync `dd04f397` (string-search in mvp_agent) | Not used by Pillow |
| `resume_session` / `close_session` | sync `ed6d543` | `resume_session_inner` refuses chat sessions and extra dirs (`session_setup.rs` ~L1428–1449, constants at top of file) |
| Baseline `acp_agent.rs` | **no** `resume_session` / `close_session` / `list_sessions` | Confirmed via `git show 5da6962e:…/acp_agent.rs \| Select-String` |

Pillow does **not** call these methods today → impact is **additive-only** unless a future design uses them.

---

### 2.2 `session/update` kinds and fields

Canonical ACP kinds still emitted through `acp::SessionUpdate` (HEAD `updates.rs` match arms ~L421–456):

| Wire kind (snake / serde) | Pillow mapper (`map/events.rs`) | Status in interval |
| --- | --- | --- |
| `agent_message_chunk` | → `AssistantDelta` | Still core |
| `agent_thought_chunk` | → `ReasoningSummaryDelta` | Still core |
| `tool_call` | → `ToolCallStarted` (+ observe) | Still core |
| `tool_call_update` | → completed/output | Still core |
| `plan` | → `PlanUpdated` | Still core |
| `user_message_chunk` | Drop (intentional B2) | Still ignored |
| `available_commands_update` | Drop | Still ignored |
| `current_mode_update` | Drop | Still ignored |
| `session_info_update` | Drop | Still ignored |

**xAI extension updates** (e.g. `subagent_spawned`, `subagent_finished`, `task_backgrounded`, `task_completed`, `goal_updated`, `response_completed`, …) continue to exist / grow on extension paths (`headless/ext_protocol.rs`, pager harness). Pillow’s mapper **drops unknown kinds** (`map/events.rs` `_ => Drop`) — additive noise only.

Fixtures exercising mapped kinds:

- `session-new-prompt-text-only.jsonl` — thought + message + `end_turn`
- `session-prompt-with-plan-and-tools.jsonl` — tool_call / tool_call_update / message
- `protocol_fixtures.rs` asserts mapper hygiene on message/thought/tool_call only

**No evidence** that core field names (`sessionUpdate`, `content.text`, `toolCallId`, `status`, `entries`/`steps`) changed in the locked `agent-client-protocol` crate.

---

### 2.3 `session/prompt` response shape and `stopReason`

| Item | Evidence | Change |
| --- | --- | --- |
| Request | `PromptRequest` with content blocks; Pillow sends `prompt: [{type:text,text}]` (`methods.rs`) | Compatible |
| Response | `PromptResponse` + `StopReason` via same crate 0.10.4 | Schema lock **unchanged** |
| Documented wire tokens (headless map HEAD `headless.rs` ~L385–391) | `end_turn`, `max_tokens`, `max_turn_requests`, `refusal`, `cancelled` | Same family as ACP schema; **not** introduced by a crate bump in this interval |
| Cancel meta on prompt result | Fixtures use `_meta.cancellationCategory` (`MidTurnAbort`, `PermissionRejected`) | Still used in fake child; upstream cancel path still returns `StopReason::Cancelled` |

Pillow `map_stop_reason` (`map/terminal.rs`):

| `stopReason` | Mapping |
| --- | --- |
| `cancelled` | `RunCancelled` |
| `end_turn`, `max_tokens` | `TurnCompleted` |
| anything else (incl. `max_turn_requests`, `refusal`) | `TurnFailed` with code `acp_stop_reason` |

This partial mapping is a **pre-existing Pillow gap** relative to the full ACP enum, **not** a regression introduced by `5da6962e..HEAD` (crate unchanged).

Fixture assertions: `session-*-*.jsonl` use `end_turn` / `cancelled` only — still valid.

---

### 2.4 Cancellation semantics

| Path | Upstream | Pillow |
| --- | --- | --- |
| Client → agent | `session/cancel` notification (`CancelNotification`) | `session_cancel_params` + `rpc.notify("session/cancel", …)` (`adapter.rs`, `prompt.rs`) — **never** `$/cancel_request` |
| Agent handler | HEAD `cancel` (~L2076–2124) still accepts sessionId; optional `_meta.cancelTrigger`, `cancelSubagents` (default true), `rewindIfNoOutput` | Pillow sends **minimal** `{sessionId}` only |
| Baseline | Same cancel method + `cancelSubagents` / `cancelTrigger` already present | Optional meta was already additive |

CHANGELOG product notes (not wire renames): stop during `/compact`, background subagent stop on stop command (`0.2.117`–`0.2.118`). Those affect agent runtime behavior under cancel, not JSON-RPC method names.

**Classification for Pillow argv/protocol:** **none** for method shape; **possible behavioral** differences if agent ignores cancel longer (Pillow already has cancel grace + kill-tree — `config.cancel_grace`).

---

### 2.5 Permission request flow

| Item | Evidence | Change |
| --- | --- | --- |
| Client method name | Still `session/request_permission` (gateway + leader + tests) | **Unchanged** |
| Params shape (fixture) | `sessionId`, `toolCall`, `options[]` with `optionId`/`kind` | Fixtures still match common shape |
| Response outcome | `outcome: { outcome: selected|cancelled, optionId? }` | Pillow `permission_result_*` unchanged |
| Agent-side policy | Large refactors under `xai-grok-workspace/src/permission/**` (auto-mode, bash classification, etc.) | **Policy**, not JSON-RPC rename |

Pillow always denies provider-native authority (`map/permission.rs` B2 policy; governed `pillow/*` tools separate). Wire still expected to be request → host decision → result.

Fixtures: `permission-request-read.jsonl`, `permission-request-write.jsonl`.

---

### 2.6 Stdio framing: NDJSON, line size, timeouts

| Item | Upstream (HEAD = baseline for lib) | Pillow |
| --- | --- | --- |
| Framing | NDJSON over stdio (`xai-acp-lib` line_reader / stdin_reader; **unchanged** in interval) | `rpc/framer.rs` NDJSON |
| Agent max line | `MAX_LINE_SIZE = 64 * 1024 * 1024` (`line_reader.rs` L25–30) | Client cap **1 MiB** (`config.rs` `DEFAULT_MAX_LINE_BYTES`) |
| Request timeout | Not a protocol constant; product choice | **180s** per request (`DEFAULT_REQUEST_TIMEOUT`) |

**Note:** Agent allows 64 MiB lines; Pillow rejects >1 MiB. That asymmetry **pre-existed** the interval (lib file unchanged). No upstream shrink of agent limit found.

---

### 2.7 CLI surface: `agent --no-leader stdio`

| Check | Result | Evidence |
| --- | --- | --- |
| Local binary | `grok 0.2.118 (1e1687c1cf)` | `grok.exe --version` |
| Local help | Subcommands: `stdio`, `headless`, `serve`, `leader`; flags `--leader`, `--no-leader` | `grok.exe agent --help` |
| Source HEAD | `AgentCmd::Stdio` + `AgentArgs.no_leader` | `crates/codegen/xai-grok-pager/src/app/cli.rs` ~L257–343 |
| Source baseline | **Same** `AgentCmd` / `no_leader` definitions | `git show 5da6962e:…/cli.rs` |
| cli.rs diff in interval | Adds unrelated top-level flags (`du`, headless partial messages, local-workspace feature flags, restore-code docs) | **Does not remove** agent stdio / no-leader |
| Runtime entry | `AgentCmd::Stdio => run_stdio_agent(...)` | `pager-bin/src/main.rs` ~L1410 |

Pillow production argv remains valid:

```text
agent --no-leader stdio
```

(`crates/adapter-grok-acp/src/config.rs` `PRODUCTION_ACP_ARGV`)

---

## 3. Q3 — Impact on Pillow adapter / fixtures (breaking / additive / none)

| Protocol area | Grade | Pillow files | Notes |
| --- | --- | --- | --- |
| `initialize` core fields | **none** | `rpc/methods.rs`, `state.rs`, `health.rs`, `initialize-ok.jsonl` | V1 + loadSession + agentVersion still valid |
| `initialize` capability ads (list/resume/close) | **additive** | none required | Extra JSON ignored by our selective reads |
| `session/new` / `session/load` | **none** | `adapter.rs`, `methods.rs`, fixtures | cwd + mcpServers + agentProfile meta still accepted |
| `session/resume` / `list` / `close` | **additive** | none (unused) | New methods; safe if not called |
| Mapped `session/update` kinds | **none** | `map/events.rs`, fixtures, `protocol_fixtures.rs` | Locked schema; unknown kinds already dropped |
| Extension updates (x.ai/*) | **additive** | mapper drops | May increase ignored traffic volume only |
| `session/prompt` + `stopReason` | **none** (schema); pre-existing **gap** for full enum | `prompt.rs`, `map/terminal.rs` | `max_turn_requests` / `refusal` → TurnFailed if ever seen |
| `session/cancel` | **none** | `adapter.rs`, `prompt.rs`, cancel fixtures | Minimal params still valid |
| `session/request_permission` | **none** (wire); agent policy **behavioral** | `map/permission.rs`, permission fixtures | Host-side deny still correct |
| NDJSON / 1 MiB / 180s | **none** | `config.rs`, `rpc/framer.rs` | Unchanged conventions |
| CLI argv | **none** | `config.rs` | Confirmed on 0.2.118 help + source |
| Governed `pillow/*` tools | **none** (Pillow-owned) | `provider_*.rs` | Not an upstream ACP method rename |

**Breaking changes that touch Pillow’s exact coupling surface:** **none found** in static analysis for the interval `5da6962e..HEAD`.

**Breaking elsewhere (did not hit our argv/protocol):** e.g. TUI-only flags, headless streaming-json features, restore-code semantics, version package string `1.0.0` in tree, large permission auto-mode behavior for interactive TUI clients.

---

## 4. Q4 — Local `grok.exe 0.2.118` vs upstream interval

| Signal | Value |
| --- | --- |
| Installed | `grok 0.2.118 (1e1687c1cf)` (stable channel label not in `--version` short form; user note: stable) |
| Public mirror sync with crate version **0.2.118** | **`780d1388fff103ff0db0d8c14de65af6225b4860`** (2026-08-03) |
| That sync’s `SOURCE_REV` | `64c4de99cc822b25ce9c54ab5a4f372093d0885d` |
| Hash `1e1687c1cf` in public history | **Not found** (not a prefix of any `SOURCE_REV` in baseline→HEAD; not in `git log` subjects) |

**Mapping confidence:**

- **Version string `0.2.118` → public sync `780d138`:** **high** (exact match of `xai-grok-version` crate + CHANGELOG date 2026-07-31 for 0.2.118 features, sync stamped 2026-08-03).
- **Build hash `1e1687c1cf` → monorepo commit:** **未能确认**. Public `SOURCE_REV` for 0.2.118 is `64c4de99…`, not `1e1687c1…`. Likely causes: shipping binary built from monorepo commit not equal to the mirror’s `SOURCE_REV` snapshot; alternate packaging hash; or channel build not published as that exact tree. **Do not treat `1e1687c1cf` as located in `xai-org/grok-build`.**

Relative placement:

```text
baseline 5da6962e (0.2.112)
    … several syncs …
780d138 (0.2.118)  ≈ local CLI version band
    … later syncs …
afbc0fb HEAD (tree version 1.0.0)
```

Local CLI is **newer than the Pillow conceptual pin (0.2.112)** and **older than HEAD (0.2.121 / 1.0.0 tree)**.

---

## 5. Q5 — Recommendations (no implementation)

### 5.1 Should the baseline pin advance?

| Option | Commit | When to choose |
| --- | --- | --- |
| **A. Advance conceptual pin to HEAD** (recommended for “latest inspected”) | `afbc0fb710320c7add294c2106d447ecc3e3af2e` + note `SOURCE_REV=3e620a76…` | Keep design docs current with public mirror; still conceptual (no code port) |
| **B. Pin near installed CLI** | `780d1388…` (0.2.118 / `SOURCE_REV=64c4de99…`) | Align docs with production binary band on this machine |
| **C. Keep `5da6962e`** | current | Only if policy is “freeze docs until live protocol acceptance re-run” |

**Suggestion:** Prefer **A** for `docs/references/upstream-baseline.md` table row update **after** design authorization, with explicit note:

- Still **conceptual reference** (no THIRD_PARTY_NOTICES code port).
- Public repo has **no release tags**.
- Tree version string at HEAD is `1.0.0` while shipping CLIs remain `0.2.x` — do not equate crate version to installer version without checking `grok --version`.

**Not recommended without further work:** treating baseline advance as automatic green light for adapter changes — static analysis shows **no required adapter code change** for current coupling.

### 5.2 If pin advances, estimated Pillow work (checklist only)

| Priority | Item | Required? |
| --- | --- | --- |
| Doc | Update `docs/references/upstream-baseline.md` commit + inspect date + optional SOURCE_REV | Yes if pin advances |
| Doc | Optional: record installed CLI `0.2.118 (1e1687c1cf)` as “runtime observed”, separate from pin | Nice |
| Adapter | Change `PRODUCTION_ACP_ARGV` | **No** |
| Adapter | Change initialize / session param builders | **No** for compatibility |
| Adapter | Implement `session/resume` / `list` / `close` | **No** unless product wants those ACP features |
| Mapper | Handle `max_turn_requests` / `refusal` stop reasons | Optional hardening (pre-existing gap) |
| Mapper | Map more x.ai session updates | Optional product surface |
| Fixtures | Add fixture lines for resume/close only if product adopts them | Optional |
| Tests | Re-run gated live native acceptance against target CLI after upgrade | **Yes** before trusting a new shipping CLI (not done in this audit) |
| Line limit | Raise 1 MiB client cap | Only if legitimate ACP messages exceed it (rare; agent allows 64 MiB) |

### 5.3 What broke elsewhere but we did not hit

- Headless streaming JSON / partial message flags (`cli.rs`) — Pillow uses ACP stdio, not headless CLI output formats.
- TUI permission UX / auto-mode classifier changes — Pillow enforces its own permission policy.
- Leader reconnect / stdio bridge multi-session replay — Pillow launches **direct** `--no-leader stdio` child, not leader bridge.
- `session/resume` chat refusal — irrelevant while we only use `session/new` + optional `session/load`.

---

## 6. Confidence

| Claim | Confidence |
| --- | --- |
| 9 syncs, no GitHub tags, HEAD `afbc0fb` | **High** (git + API) |
| `agent-client-protocol` 0.10.4 + schema 0.11.4 unchanged; `xai-acp-lib` byte-identical path | **High** |
| Core methods initialize / new / load / prompt / cancel / request_permission still present | **High** |
| resume / list / close newly implemented on agent in interval | **High** (string search + baseline absence) |
| CLI `agent --no-leader stdio` stable | **High** (source + local help) |
| No wire breaking change for Pillow’s selective field use | **High** (static); **Medium** for runtime behavioral cancel/auth edge cases without live session |
| Local `0.2.118` ↔ sync `780d138` | **High** for version string; **Low** for hash `1e1687c1cf` (not located) |
| Overall “safe to keep adapter as-is on 0.2.118” | **High** for protocol shape; live acceptance still the release gate |

---

## 7. Evidence index (paths + commits)

| Topic | Path / ref |
| --- | --- |
| Pillow pin | `<repo>/docs/references/upstream-baseline.md` |
| Pillow argv / limits | `crates/adapter-grok-acp/src/config.rs` |
| Pillow RPC builders | `crates/adapter-grok-acp/src/rpc/methods.rs` |
| Pillow update map | `crates/adapter-grok-acp/src/map/events.rs` |
| Pillow stopReason map | `crates/adapter-grok-acp/src/map/terminal.rs` |
| Pillow fixtures | `tests/fixtures/protocol/grok-acp/*`, `tests/protocol_fixtures.rs` |
| Upstream HEAD | `afbc0fb710320c7add294c2106d447ecc3e3af2e` |
| Upstream baseline | `5da6962e4adb9c857f3def762542b52b4ec3e522` |
| ACP crate lock | `Cargo.lock` `agent-client-protocol` 0.10.4 (both ends) |
| Transport lib | `crates/codegen/xai-acp-lib/src/line_reader.rs` (64 MiB; unchanged) |
| Agent trait | `crates/codegen/xai-grok-shell/src/agent/mvp_agent/acp_agent.rs` |
| Session setup split | `…/mvp_agent/session_setup.rs` (from `ed6d543`) |
| CLI defs | `crates/codegen/xai-grok-pager/src/app/cli.rs` |
| Changelog versions | `crates/codegen/xai-grok-shell/CHANGELOG.md` |
| Version crate timeline | `crates/codegen/xai-grok-version/Cargo.toml` per sync |

---

## 8. Explicit non-actions

- **未执行上游代码** (no build, no install, no scripts).
- **未启动真实 ACP 会话** (no interactive `grok agent stdio`, no quota burn).
- **未修改** product code or baseline pin (only this report under `docs/execution/`).
- **未** `git add` / commit; **未** 读 auth/token; 临时克隆用后删除。

---

## 9. Bottom line for 总控

1. Public mirror moved **9 monorepo syncs** past Pillow’s pin; **no release tags**.  
2. **ACP wire crate and stdio transport library are unchanged** across the interval.  
3. Agent gained **additive** session lifecycle methods (list/resume/close) and internal refactors; Pillow’s used surface (initialize, new/load, prompt, cancel, request_permission, NDJSON stdio, `agent --no-leader stdio`) remains **compatible**.  
4. Local **0.2.118** aligns with public sync **`780d138`**; build hash **`1e1687c1cf` 未能确认** in public history.  
5. Recommend doc pin advance to **`afbc0fb`** (or `780d138` if locking to current CLI band) **only after design authorization**; **no mandatory adapter code change** for the current coupling; optional stopReason completeness and live re-acceptance when upgrading CLI past 0.2.118.

`UPSTREAM-GROKBUILD-AUDIT-COMPLETE`
