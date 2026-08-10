# Pillow — 治理工具如何告知模型：优先复用 Grok Build / ACP 现成机制

Date: 2026-08-10  
Status: **Research only — no product code change**  
Controller: Claude  
Worker: research (read-only)  
Completion token: `TOOL-EXPOSURE-REUSE-COMPLETE`

## 0. Scope and hard constraints

| In | Out |
| --- | --- |
| What Grok Build / ACP already provides to expose tools to the model | Full design or implementation |
| Whether self-built `pillow/*` can be replaced by those mechanisms | Running cargo / pnpm / gates / E2E |
| How governance (proposal → approve → host exec → receipt) could hang on | Re-arguing why native tools are disabled |
| Impact on permission-rules minimal slice | Reading credentials (`auth.json`, tokens) |

Hard rules observed for this brief:

- No source / test / script / config edits other than this report.
- No builds or concurrent `target/` use.
- No machine absolute paths; credentials never opened.
- Speculation labeled **未确定**; verified items labeled **查证**.

Background taken as given (not re-proven):

- Production profile sends `"tools": []` and a long `disallowedTools` list (`crates/adapter-grok-acp/src/rpc/methods.rs` → `plaintext_agent_profile()`).
- Native tools must stay off so Grok cannot bypass governance with `write` / `bash` / etc.
- Adapter handles `pillow/write_tool` / `pillow/process_tool`, but only the test `fake_acp_agent` emits them.
- Net: real Grok under Pillow has no tools; C3/C4 acceptance is fake-driven.

User direction (binding): **prefer Grok Build mechanisms; do not invent a parallel tool-declaration protocol.**

---

## 1. One-line answer

**The only standard, client-driven way to hand *new* tools to the Grok agent is ACP `session/new|load.mcpServers` (plus Grok’s MCP naming / `search_tool`+`use_tool` surface).**  
There is **no** ACP field for “here is an arbitrary client tool catalog.”  
`agentProfile.tools` only **filters built-in** tools.  
Self-built `pillow/*` JSON-RPC is **not** something real Grok will call.

---

## 2. Sources consulted (read-only)

| Source | Use |
| --- | --- |
| Local Grok user guide (`README.md` under Grok config root): MCP Servers, Tool Naming, Built-in Tools, Controlling Available Tools, Permission Rules, Agent Mode / ACP samples | Primary product surface |
| Local Grok `bundled/agents/*.md` (frontmatter; no custom tool schemas) | Agent profile shape |
| Public ACP docs: Session Setup, Overview, Schema fragments (stdio / HTTP / SSE MCP) | Protocol contract |
| Pillow: `rpc/methods.rs`, `prompt.rs`, `provider_*.rs`, `map/permission.rs`, fake agent | What we send / handle today |
| Prior research: `docs/research/2026-07-30-grok-acp-protocol.md`, `2026-08-10-agent-context-gap.md`, `docs/execution/2026-08-08-upstream-grokbuild-audit.md` | Live probe + wire continuity |
| Permission design: `docs/superpowers/specs/2026-08-10-permission-rules-design.md` | `MCPTool(...)` gap row |

**Not read:** any auth / credential store.

---

## 3.1 Mechanism inventory

Each row answers: shape → model-side effect → Pillow status. Status is **查证** or **未确定**.

### A. ACP `session/new` / `session/load` field `mcpServers`

| Question | Answer | Status |
| --- | --- | --- |
| Do we already send it? | Yes, always `[]` in `session_new_params` / `session_load_params`. Tests assert “ACP requires the field, but no MCP server may be attached.” | **查证** (Pillow code) |
| Protocol shape? | Required array. Entries are MCP connection configs the **agent** connects to. | **查证** (ACP Session Setup + Schema) |
| Stdio transport (MUST on all agents) | `{ name, command, args, env? }` — `command` is absolute path to MCP executable; agent spawns it. | **查证** (ACP) |
| HTTP transport (optional) | `{ type: "http", name, url, headers }` when agent advertises HTTP MCP. | **查证** (ACP) |
| SSE transport (optional) | `{ type: "sse", name, url, headers }` when advertised. | **查证** (ACP) |
| Does Grok agent support HTTP MCP? | Live initialize result included `agentCapabilities.mcpCapabilities: { http: true, sse: true }` (protocol probe, Grok Build 0.2.112 era). | **查证** (prior live research) |
| What happens after non-empty list? | Agent **must** connect to listed servers; tools from those servers become available to the agent runtime (MCP client role). Client does **not** implement the tool call on the ACP JSON-RPC line for those tools. | **查证** (ACP Session Setup wording) |
| What does the **model** see? | Grok namespaces MCP tools as `serverName__toolName`. Built-ins `search_tool` / `use_tool` exist to discover / invoke integration (MCP) tools. Exact injection path (first-class tool list vs only via search/use) is product-internal. | **查证** (README naming + built-in table); first-class vs indirection details **未确定** without live tool listing |
| Pillow today | Empty list + `search_tool` / `use_tool` on `disallowedTools` → MCP path fully closed even if config elsewhere had servers. | **查证** |

### B. `_meta.agentProfile.tools` / `disallowedTools`

| Question | Answer | Status |
| --- | --- | --- |
| What is it? | Grok-specific session meta (not core ACP schema). Client-supplied profile; still parsed upstream (audit). | **查证** (Pillow methods + upstream audit) |
| What does `tools` accept? | Allowlist of **built-in tool IDs** (README agent-profile frontmatter: `read_file`, `grep_search`, `list_dir`, …). Headless `--tools` same idea. | **查证** (README) |
| Can the client declare custom tools / schemas here? | **No evidence.** Documented values are built-in IDs only. Bundled agents do not embed tool schemas in frontmatter. | **查证** that docs only show built-ins; **未确定** whether unknown strings are silently ignored or error |
| Empty `tools: []` | Consistent with “no allowlist / no provider tools.” Pillow tests require empty tools. | **查证** (Pillow intent + tests) |
| `disallowedTools` | Denylist of built-in names; Pillow lists native FS/shell/web/memory/subagent/MCP discovery tools. | **查证** |
| Can this alone expose governance tools? | **No** — it only filters Grok’s existing registry; it does not register host-owned tools. | **查证** (by mechanism shape) |

### C. MCP tool naming (`server__tool`)

| Question | Answer | Status |
| --- | --- | --- |
| Rule | Server `filesystem` + tool `read_file` → `filesystem__read_file`. | **查证** (README §Tool Naming) |
| Meaning for Pillow | If governance tools are MCP tools under server name e.g. `pillow`, model-facing names look like `pillow__write` / `pillow__process` (exact tool IDs are ours to define). Permission prefix `MCPTool(...)` then has a natural subject string. | **查证** (naming rule); exact Pillow tool IDs **not chosen** (out of scope) |
| Discovery tools | Built-ins `search_tool` / `use_tool` are documented for MCP integration tools. Production profile **disallows both**. | **查证** |
| Interaction with empty `tools: []` | Empty allowlist may strip defaults; whether MCP tools still appear after connect when allowlist is empty is **未确定** without a live probe. Safe assumption for planning: you must **revisit** `tools` / `disallowedTools` so MCP invocation is possible without re-enabling native `write` / `bash`. | **未确定** (product behavior) |

### D. `~/.grok/config.toml` / project `.grok/config.toml` `[mcp_servers]`

| Question | Answer | Status |
| --- | --- | --- |
| Config shape | `[mcp_servers.<name>]` with `command`/`args`/`env` or HTTP `url`/`headers`; timeouts; `enabled`. | **查证** (README) |
| Scopes | User config lowest; walk from cwd to repo root; same name **replaces** (not merge). Project file only supports `[mcp_servers]`. | **查证** |
| Relation to ACP `mcpServers` | Both are ways to attach MCP servers to a Grok session. README ACP samples always show `mcpServers: []` and do not document merge rules with config.toml for `agent stdio`. | **未确定** (merge / override order under ACP) |
| Pillow product path | Isolated profile home for agent child; product deliberately does not attach MCP; wave briefs kept `mcpServers=[]` until a Pillow-owned gateway is proven. | **查证** (methods + B3 brief + context-gap) |
| Prefer which for product? | **Session `mcpServers`** is the client-controlled, per-session contract (ACP). Config.toml is ambient / user-machine and harder to scope to project isolation + host authority. | Recommendation (not live-proven merge) |

### E. ACP “client provides tools” without MCP

| Mechanism | Role | Status |
| --- | --- | --- |
| `clientCapabilities.fs.readTextFile` → agent calls `fs/read_text_file` / `fs/readTextFile` | Agent → **client** reverse RPC; Pillow already normalizes to governed `fs.read@1`. | **查证** |
| `clientCapabilities.fs.writeTextFile` → `fs/write_text_file` | Standard write reverse RPC. Pillow currently **advertises false** and does not implement write reverse path as C3 entry. | **查证** (capability false); whether Grok would use it when native write is disallowed **未确定** (prior probe did not exercise true write capability) |
| `clientCapabilities.terminal` → `terminal/*` | Free terminal/shell surface. Maps poorly to C4 **catalog + structured argv** process governance. | **查证** (ACP has terminal; Pillow process model is not free shell) |
| `session/request_permission` | Agent asks client to allow a **provider-native** tool call. Pillow **always rejects** (`decide_permission` → `allowed: false`). | **查证** |
| Arbitrary client tool catalog on `session/new` | **Not in ACP.** Only `mcpServers` + capabilities above. | **查证** (ACP Overview + Session Setup) |
| Custom methods | ACP extensibility: custom methods should use `_` prefix; `_meta` for metadata. Pillow’s `pillow/*` is a **non-standard** server→client method family only the fake agent emits. | **查证** (ACP extensibility note + fake agent + real Grok never documented to call `pillow/*`) |

### 3.1 summary table

| Mechanism | Client can hand tools? | Model sees? | Governance-friendly? | Status |
| --- | --- | --- | --- | --- |
| `mcpServers` (stdio/HTTP/SSE) | Yes — by spawning/connecting MCP | Namespaced MCP tools (+ Grok search/use) | Yes if MCP impl is host-governed | **查证** shape; model listing details **未确定** |
| `agentProfile.tools` | No — filter built-ins only | Built-in subset | Only for *removing* native tools | **查证** |
| config.toml `mcp_servers` | Ambient attach | Same as MCP | Weaker project isolation unless carefully scoped | Shape **查证**; ACP merge **未确定** |
| Client `fs/*` / `terminal/*` | Limited reverse RPCs | Via agent’s decision to call client | Write maybe; process **no** clean map | **查证** methods; Grok fallback behavior **未确定** |
| Custom `pillow/*` | Only if agent knows to call them | Only if agent emits them | Host already gates them | Handler **查证**; real Grok emit **no evidence** |

---

## 3.2 Governance mount: how approval stays in the host

Required pipeline (must not be bypassed):

```text
proposal → one-shot human/host decision → host execute → durable receipt → project isolation
```

### Option 1 — MCP server owned by Pillow (preferred reuse axis)

| Question | Feasible answer | Status |
| --- | --- | --- |
| Who is the MCP server? | A **Pillow-owned** MCP surface that exposes only governed tools (e.g. write / process / maybe read), **not** a generic filesystem MCP. | Design inference on **查证** MCP model |
| Where does it run? | **(a)** stdio: agent spawns a binary/command Pillow supplies; process talks back to host via IPC (pipe/socket/env token). **(b)** HTTP: host process listens on loopback; session passes `{ type:"http", url, headers }` — Grok advertised HTTP MCP support. | **查证** transports + Grok `mcpCapabilities.http` |
| Extra process? | Stdio MCP ⇒ **yes**, agent-spawned child. HTTP MCP on the **host process** ⇒ **no separate MCP binary** if the host itself is the HTTP MCP endpoint (still a network listener). In-process “fake MCP without transport” is **not** an ACP standard. | **查证** transports; “host is HTTP MCP” is **feasible by protocol**, not yet implemented |
| How does a tool call hit approval UI + receipt? | Agent (MCP client) → MCP tool call → Pillow MCP handler → existing host ports (`GovernedExecutionPort` / proposal ledger / UI approval / apply). MCP response returns only after decision + execute (or structured rejection). | Logical path; **not** live-proven |
| Does `prompt.rs` `pillow/*` remain usable? | **Normalization + response shaping + governance calls** stay valuable. Wire entry becomes MCP tool names/params instead of JSON-RPC method `pillow/write_tool`. Fake agent would need to speak MCP or keep dual entry for tests. | Inference from code structure |
| Permission for MCP? | If Grok still fires `session/request_permission` for MCP tools, current **always-reject** policy would block the call **before** MCP runs. Whether MCP tools request permission is **未确定**. | Policy **查证** reject-all; MCP permission behavior **未确定** |
| Must still deny | Native `write` / `search_replace` / `bash` / free terminal / generic filesystem MCP that writes outside ledger. | Hard constraint |

**Feasibility verdict:** **Yes, this is the protocol-aligned reuse path.** Governance does not move into Grok; Grok only *invokes* tools whose implementation is host-owned MCP.

### Option 2 — Standard client reverse RPCs only (`fs/write_text_file`, `terminal/*`)

| Aspect | Assessment |
| --- | --- |
| Write | Enabling `writeTextFile` and implementing `fs/write_text_file` **could** map into C3 write proposals **if** real Grok calls client write when native write is disallowed. That fallback is **未确定**. |
| Process | ACP `terminal/*` is not C4 catalog+argv; using it would reopen free-shell risk or require a non-standard terminal policy. **Poor fit.** |
| Reuse of `pillow/*` | Read path already dual-handles ACP fs read + `pillow/read_tool`. Write would be a new reverse method, not MCP. |
| Verdict | Partial, write-only possibility; **not** a complete replacement for write+process tool exposure. |

### Option 3 — Keep `pillow/*` and teach the model in prompt text

| Aspect | Assessment |
| --- | --- |
| Real Grok | No product contract that Grok will invent `pillow/write_tool` JSON-RPC. | 
| Verdict | **Not a reuse of Grok Build.** Only works with a custom agent binary (current fake). Rank last. |

### What is **not** allowed

- Re-enabling native `write_file` / `bash` and “trusting” `session/request_permission` — Pillow rejects those and must not grant provider-native authority.
- Pointing Grok at an unrestricted filesystem MCP as a shortcut.
- Treating empty `tools: []` as “tools already exposed.”

---

## 3.3 Relation to permission-rules design

Source: `docs/superpowers/specs/2026-08-10-permission-rules-design.md`.

| Topic | Implication |
| --- | --- |
| `MCPTool(...)` marked “capability gap” | Correct **today** (no MCP). If governance tools become MCP tools, the prefix becomes a **real match class**, not a missing feature to invent later. |
| Minimal slice (Write / Edit / Bash→process) | **No conflict.** That slice targets C3/C4 proposals already on the ledger. MCP exposure is about **how the model invokes** those proposals, not about replacing rule evaluation. |
| Should minimal slice wait for MCP? | **No.** Click-reduction rules apply once proposals exist; fake or real entry still hits the same pending bar. |
| If MCP lands before/with rules | Map `MCPTool(pillow__*)` (or chosen names) as **optional** extra subjects; do **not** drop Write/Edit/Bash mapping — those remain the domain subjects on proposals. Design already said: *if MCP lands, match subject = host registry tool name string.* |
| Risk of double gating | Grok permission rules / `session/request_permission` **plus** Pillow rules **plus** ApproveOnce. Product must keep **one** user-visible authority: Pillow ledger. Auto-approve of provider-native tools remains forbidden. |
| Project allow rules | Unchanged: never grant allow from project tree; MCP must not become a backdoor for project-authored authority. |

**Summary:** Enabling MCP-shaped governance tools **closes** the design’s `MCPTool` gap row; it does **not** force redesign of the Write/Edit/process minimal slice. Adjust docs when MCP ships; no inherent contradiction.

---

## 3.4 Self-built vs Grok-ready comparison

| 自建 (Pillow today) | Grok / ACP 现成 | 可替换? | 代价 |
| --- | --- | --- | --- |
| `pillow/write_tool` JSON-RPC | MCP tool e.g. `pillow__…` via `mcpServers` | **可替换入口** | New MCP surface + session params; rewire handler entry; keep ledger |
| `pillow/process_tool` JSON-RPC | Same MCP server, process tool | **可替换入口** | Same; catalog/argv validation stays host-side |
| `pillow/read_tool` | ACP `fs/read_text_file` (already) **or** MCP read tools | Partial already via ACP fs | Prefer keep ACP fs read; MCP optional |
| Fake agent emits custom methods | Real Grok emits tools only for built-ins + MCP | Fake remains for unit/integration | Live proof needs real Grok + MCP |
| `tools: []` + long `disallowedTools` | Same profile filters (built-in control) | **Keep** for natives | Must carefully allow MCP path without reopening write/bash |
| `mcpServers: []` | Non-empty stdio/HTTP MCP list | **Must change** to expose tools | Session attach + lifecycle |
| Always-reject `session/request_permission` | Grok permission for native tools | **Keep reject for natives** | May need exception or proof that MCP does not use this RPC |
| Config.toml `[mcp_servers]` ambient | Session `mcpServers` preferred | Prefer session field | Avoid ambient config as product authority |
| `agentProfile.tools` custom catalog | Not supported | **不可** as custom tool registry | Do not invent schemas here |
| Prompt text listing tools | Not a Grok “tool registry” | Unreliable | Not a substitute for MCP/ACP |

---

## 4. Minimal verification path

Goal: prove **once** that **real Grok** (not fake) can invoke **one** governed tool that still does proposal → approve → host exec → receipt.

Ordered by **reuse of Grok/ACP** (self-built last).

### Path A — Highest reuse: session `mcpServers` + host MCP (HTTP preferred)

**Minimal moves (conceptual only):**

1. Advertise a Pillow MCP endpoint on session create (prefer **HTTP loopback** so Grok’s advertised `mcpCapabilities.http` is used and host process can be the server).
2. Expose **one** tool (start with governed write or a no-op-ish process catalog label — pick the simpler end-to-end path).
3. Keep native write/bash **disallowed**.
4. Adjust profile just enough that MCP tools are invocable (likely allow `search_tool`/`use_tool`, or confirm MCP tools are first-class; **must be live-checked**).
5. MCP handler calls existing governance port; UI approve once; assert receipt row.
6. Confirm `session/request_permission` does not hard-block the MCP tool (if it does, fix only after measuring).

**Why first:** Uses the **documented client→agent tool extension** (MCP). No custom agent binary. Aligns with user “don’t reinvent.”

### Path B — ACP client `fs/write_text_file` only (write probe)

1. Set `writeTextFile: true` and implement reverse write → C3 proposal path.
2. Keep native write disallowed; prompt model to write a file.
3. Observe whether real Grok actually calls client fs write.

**Why second:** Standard ACP, no MCP process. **Incomplete** (no process tool) and **behavior 未确定**.

### Path C — Ambient config.toml MCP under isolated profile home

Same as A but attach server via profile config instead of session field.

**Why later:** Weaker session scoping; merge with ACP list **未确定**; worse product isolation story.

### Path D — Self-built only: teach real Grok to call `pillow/*`

1. System/user prompt or agent profile text describing custom methods.
2. Hope model emits non-standard JSON-RPC.

**Why last:** **Not** Grok Build’s tool mechanism. No evidence real Grok will call `pillow/*`. Only justified if MCP and client-fs paths are proven impossible (they are not, on paper).

### Smallest “one green bar” definition

- Real `grok agent … stdio` child (production argv family).
- One successful governed write **or** process with SQLite proposal + decision + receipt.
- Native tools still cannot write/exec outside that path.

---

## 5. What this brief did **not** verify

| Item | Why blocked / skipped |
| --- | --- |
| Live attach of non-empty `mcpServers` to real Grok | No live agent run this ticket (build concurrency / read-only) |
| Exact model-visible tool list with MCP connected under `tools: []` | Needs live session / inspect |
| Whether MCP tool calls hit `session/request_permission` | Needs live |
| Merge order: ACP `mcpServers` vs config.toml vs Claude-compat `.mcp.json` under agent stdio | Not documented clearly in README for ACP mode |
| Whether unknown `agentProfile.tools` entries are ignored | No source / live |
| Whether disabling natives causes automatic fallback to `fs/write_text_file` | Explicitly 未确定 in prior research; not re-probed |
| Full MCP tool JSON schemas Grok expects at connect | Standard MCP; not re-derived here |

---

## 6. Uncertainties (carry forward)

1. **Empty `tools: []` + MCP:** does Grok still register MCP tools?
2. **Must `search_tool` / `use_tool` be allowed** for MCP, or are `server__tool` names direct?
3. **Permission RPC for MCP:** reject-all may brick Path A.
4. **HTTP MCP + isolated agent env:** loopback URL, session headers (`{{session_id}}` exists in README HTTP example), auth between agent and host MCP.
5. **Stdio MCP spawn under Pillow’s locked-down agent env:** path, env allowlist, project isolation.
6. **Dual entry** during migration: keep `pillow/*` for fake tests while production uses MCP?

---

## 7. Conclusions (only what is justified)

1. **Reuse answer:** Hand tools to Grok via **ACP `mcpServers`** (stdio or HTTP). That is the existing client extension point.  
2. **`agentProfile.tools` is not a custom tool API** — only built-in filtering.  
3. **`pillow/*` is adapter-local**; real Grok will not emit it; replace the *entry* with MCP (or possibly ACP fs write for writes only), keep host governance.  
4. **Host can avoid an extra MCP binary** by being an **HTTP MCP** endpoint (Grok advertised HTTP support); stdio always implies agent-spawned process.  
5. **Permission-rules minimal slice** does not conflict; `MCPTool(...)` becomes real when MCP lands.  
6. **Do not** re-enable native write/bash to “get tools working.”

---

## 8. Report hygiene

- No product code, tests, configs, or scripts modified.
- No cargo / pnpm / gates / E2E run.
- No credentials read.
- No machine absolute paths in this document.

**TOOL-EXPOSURE-REUSE-COMPLETE**
