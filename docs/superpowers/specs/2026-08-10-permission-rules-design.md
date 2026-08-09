# Pillow — Permission Rules Layer Design

Date: 2026-08-10  
Status: **Design only — not authorized for implementation**  
Controller: Claude  
Worker: design (read-only)  
Work directory: `<repo>`

## 0. Scope and hard constraints

This document maps **Grok’s permission-rules model** onto Pillow’s existing
governed proposal / grant / receipt machinery. It invents **no parallel safety
framework**.

| In scope | Out of scope |
| --- | --- |
| Grok → Pillow prefix mapping and match subjects | Any product code change |
| Storage / global vs project tighten-only | OS sandbox (Landlock / Seatbelt) |
| Receipt fields for rule-based decisions | MCP / WebFetch product design |
| UI shape (design) | Running cargo / pnpm / gates / E2E |
| Minimal delivery slice | Project-directory config files as authority |

Controller-fixed rules (must not be reversed):

1. **Rules decide whether to ask; they do not remove audit.** Every real
   execution still produces a durable receipt. Auto-approve ≠ unlogged.
2. **Allow rules may only come from user-level (global) authority.** Project
   scope may only **tighten** (deny). **Never** authorize from a file under the
   project tree (malicious project must not grant itself `Bash(*)`).
3. **Deny beats allow.** No rule match ⇒ **require confirmation** (not auto-pass).

Sources consulted (local only unless noted):

- Local Grok user guide: `README.md` under the Grok config root
  (Permission Rules; Sandbox).
- Local bundled role/agent shapes under Grok `bundled/{roles,agents,personas}`.
- Pillow: `crates/kernel-policy/src/{write,process}.rs`,
  `crates/storage-sqlite/src/settings.rs`, migrations
  `0005_write_proposals.sql` / `0007_exec_proposals.sql` / `0009_settings.sql`,
  ADR-0003, C3/C4 designs, wave C4 checkpoint.
- Optional public reference: search snippets and partial page content for
  `xai-org/grok-build` user-guide `22-permissions-and-safety.md` (not cloned;
  details that could not be fully verified are marked **未确定**).

---

## 1. Problem statement (one line)

Pillow today only exposes `ApproveOnce` / `Reject` for governed write and
process proposals, so a real development task forces many serial human clicks
and is not usable for day-to-day agent coding. Users want Grok-shaped
**permission rules** as a convenience gate **on top of** Pillow’s auditable
authorization, not instead of it.

---

## 2. Grok model summary

### 2.1 Verified from local Grok README

| Topic | What README states (查证) |
| --- | --- |
| Purpose | Rules control whether a tool invocation is **auto-approved**, **denied**, or **requires confirmation**. Unlike `--disallowed-tools`, rules **leave tools available** and only gate execution. |
| Syntax | `ToolPrefix(glob_pattern)`. Bare prefix (no parentheses) matches **all** invocations of that type. |
| Prefixes | `Bash(...)`, `Edit(...)`, `Write(...)`, `Read(...)`, `Grep(...)`, `WebFetch(...)`, `MCPTool(...)`. |
| Glob | Supports `*` and `**`. |
| Bash compatibility | Claude Code form `Bash(cmd:*)` is accepted and is **equivalent to prefix matching on `cmd`**. |
| Priority | **Deny rules take precedence over allow rules.** |
| Surfaces | `--allow` / `--deny` (repeatable); work in TUI and headless. |
| Distinct layer: Sandbox | OS-level FS/network isolation (Landlock Linux / Seatbelt macOS). Profiles: `off`, `workspace`, `read-only`, `strict`. Applied to the **whole process at startup**, irreversible. **Not** the same as permission rules. |
| Config trust (auth helpers) | Provider auth commands are honored only from **trusted config layers** (user config, managed config, requirements). A project’s `.grok/config.toml` **can never define** a provider auth command. |

### 2.2 Observed from public Grok Build docs (partial; mark gaps)

The public user-guide page *Permissions and safety* (Apache-2.0 tree under
`xai-org/grok-build`) adds detail **beyond** the local README. Only items that
appeared clearly in fetched page content are treated as 查证; the rest stay
未确定.

| Topic | Status | Content |
| --- | --- | --- |
| Authorization pipeline order | 查证 (public guide) | PreToolUse hooks → permission rules → remembered grants → built-in auto-approvals → prompt policy (mode). |
| Rule action priority | 查证 (public guide) | **deny > ask > allow** (README only documents deny > allow; `ask` is an explicit third action in the guide). |
| Default read-only tools | 查证 (public guide) | `read_file`, `list_dir`, `grep`, `web_search`, `todo_write`, some subagent control, skill invoke — **do not prompt by default** unless deny/ask/hook blocks. |
| Built-in read-only shell allowlist | 查证 (public guide) | Fixed command list (e.g. `ls`, `git status`, …) after chain splitting; word-boundary matched. Distinct from user `Bash(...)` rule matching. |
| Bash rule matching | 查证 (public guide, summary) | A `Bash(...)` pattern matches a command by **prefix** (character-for-character start) **or** by **glob on the whole command string**. No word-boundary: `Bash(git)` also matches `gitleaks…`. Trailing space + wildcard (`Bash(git *)`) forces a whole-word-ish prefix. `*` may match spaces/slashes in glob form. |
| Chained shell segments | 查证 (public guide) | Commands split on `&&`, `||`, `;`, pipes for built-in read-only and for deny/ask segment checks; **allow** matching on whole-string has additional caveats in secondary sources. |
| Always-approve vs rules | 查证 (public guide) | Always-approve skips ordinary prompts; **deny**, hooks, and some shell **ask** segment rules still apply. |
| Project config may carry allow rules | 查证 (public guide) | Grok merges rules from `~/.grok/config.toml`, project `.grok/config.toml`, and Claude-compatible settings. **Pillow deliberately does not copy “project allow”.** |
| Exact glob engine | 未确定 | Whether `*` is single-segment vs full string, case folding, path separators on Windows, and normalization before match are not fully fixed from README alone; public guide text is incomplete in the fetch used for this design. |
| Exact `Write` vs `Edit` subject | 未确定 | README maps both to path globs; whether create-only vs modify-only is distinguished, and whether content is part of the match subject, is 未确定. |
| Exact `MCPTool` / `WebFetch` match subject | 未确定 for Pillow | Pillow has neither capability; not needed for mapping beyond gap rows. |
| Secondary clone sources | 未确定 | Third-party mirrors (e.g. open-grok notes) mention “allow matches whole string only” for Bash; that may or may not match current Grok Build. **Do not treat as binding.** |

### 2.3 Layers Pillow must keep separate

```text
┌─────────────────────────────────────────────────────────────┐
│  A. Permission rules (this design)                          │
│     Convenience gate: auto-approve / deny / ask             │
│     Does NOT replace receipts or grants                     │
├─────────────────────────────────────────────────────────────┤
│  B. Proposal + human/host decision + grant + receipt (C3/C4)│
│     Auditable authorization (already shipped)               │
├─────────────────────────────────────────────────────────────┤
│  C. Catalog / path policy / env denylist (C3/C4)            │
│     Structural hard limits (program catalog, denied paths)  │
├─────────────────────────────────────────────────────────────┤
│  D. OS sandbox (Grok has it; Pillow does not)               │
│     Kernel FS/network isolation — future optional layer     │
└─────────────────────────────────────────────────────────────┘
```

Grok’s sandbox is **not** a substitute for rules, and rules are **not** a
substitute for sandbox. Pillow may later add D without reopening A–C.

---

## 3. Pillow status quo (relevant facts)

### 3.1 Decisions are binary and interactive only

| Surface | Decision enum | Meaning |
| --- | --- | --- |
| Write (`kernel-policy` write) | `ApproveOnce` \| `Reject` | Human only; no rule source |
| Process (`kernel-policy` process) | `ApproveOnce` \| `Reject` | Human only; no rule source |

Both apply only from `PendingApproval`, produce one decision row, and drive
`Approved` vs `Rejected`. There is **no** “session always allow”, no path
prefix remember, and no rule evaluator.

### 3.2 Tools that exist today

| Pillow tool | Human approval bar today | Durable proposal ledger |
| --- | --- | --- |
| `fs.write@1` | Yes (C3) | `write_proposals` + `write_approval_decisions` |
| `process.run@1` | Yes (C4) | `exec_proposals` + `exec_approval_decisions` |
| `fs.read@1` / `fs.list@1` / `fs.search@1` | No interactive ApproveOnce (governed execute + outcome/receipt path) | Not the C3/C4 approval-bar shape |
| Web fetch / MCP | **Absent** | — |

Pain is concentrated on **write** and **process** clicks during agent turns.

### 3.3 Process matchable fields (C4)

`process.run@1` is **not** a free shell string:

- `program`: host **catalog label** (public catalog currently
  `cargo`, `rustc`, `node`, `python`, `python3`; many labels denied including
  shells and package managers).
- `argv`: structured literal arguments (never joined for execution; host may
  build a **display string** for UI only).
- `cwd`: optional portable project-relative path.

Design implication: Grok’s `Bash(...)` semantics map onto a **synthetic
command subject** Pillow defines, not onto an OS shell line. Getting this
wrong either fails to auto-approve anything useful or over-approves.

### 3.4 Write matchable fields (C3)

- Portable project-relative `path`
- `mode`: `create` | `modify`
- Content digests (not content) in SQLite

Path policy already hard-denies sensitive components (`.git`, `.pillow`,
credential-like leaves, etc.). Rules sit **above** that structural policy.

### 3.5 Settings authority (S0 / ADR-0003)

- Durable non-secret settings live in SQLite `settings_entries`
  (global + project).
- Precedence for ordinary settings: `env → project → global → default`.
- Settings edits are **user-origin** and do **not** go through C3 proposals
  (ADR-0003 §3).
- Project settings already cannot inject secret or process env; **this design
  must not open a hole for project-scoped allow rules**.

### 3.6 C3/C4 governance boundary (checkpoint)

Wave C4 checkpoint: approve-once process + Git **read** UI; no interactive
shell; no MCP; no arbitrary argv beyond catalog. Permission rules must not
re-open those forbids.

---

## 4. Target model (Pillow)

### 4.1 One sentence

**Evaluate a small, Grok-shaped rule set at proposal pending time; on allow,
host-auto-transition with a rule-attributed decision receipt; on deny,
terminal reject with rule attribution; on no match, keep today’s human
ApproveOnce / Reject.**

### 4.2 Decision outcomes after rules

| Rule evaluation | Proposal effect | User interaction |
| --- | --- | --- |
| Matching **deny** | Terminal `Rejected` (or explicit `RejectedByRule` label if enum extended) | No bar; timeline shows denied-by-rule |
| Matching **allow** (and no deny) | Transition to `Approved` as if approved once | No bar; timeline shows auto-approved-by-rule |
| No match | Stay `PendingApproval` | Existing approval bar |
| Matching **ask** (optional later) | Force prompt even if a broader allow would have auto-passed | Same bar as today |

Minimal slice can ship **only allow + deny** (Grok README surface). The public
Grok `ask` action is **compatible** but not required for the first cut.

### 4.3 Still mandatory under auto-approve

- Proposal row, fingerprint, expiry rules, CAS / re-seal checks.
- One decision per proposal.
- Grant/digest discipline for apply/spawn (host may mint a **host-private
  synthetic grant** for rule decisions so the apply path stays uniform —
  implementation choice; design requires digest + no raw grant in SQLite).
- Terminal receipt with outcome metadata.
- Restart still interrupts non-terminals that lose sealed memory.

**Auto-approve never skips the ledger.**

---

## 5. Prefix mapping table (§4.1 deliverable)

### 5.1 Mapping

| Grok prefix | Pillow primitive | Match **subject** (exact definition) | Notes |
| --- | --- | --- | --- |
| `Bash(...)` | `process.run@1` proposal | **Canonical process subject string** (see §5.2) | Not OS shell; catalog label + structured argv |
| `Write(...)` | `fs.write@1` proposal with `mode = create` | **Portable path** as stored on the proposal (`path` column; project-relative, forward-slash portable grammar) | Aligns with “write new file” |
| `Edit(...)` | `fs.write@1` proposal with `mode = modify` | Same portable path | Aligns with “edit existing file” |
| `Write` / `Edit` bare | All create / all modify writes | Entire class | Same as Grok bare prefix |
| `Read(...)` | `fs.read@1` (and optionally `fs.list@1` path) | Portable path | **Gap for interactive gate:** reads already run without ApproveOnce; rules matter only if Pillow later adds read prompts or deny-on-read |
| `Grep(...)` | `fs.search@1` | Search root path (default `.`) | Same gap as Read |
| `WebFetch(...)` | **None** | — | **Capability gap** — record only |
| `MCPTool(...)` | **None** | — | **Capability gap** — record only |

### 5.2 `Bash` / process match subject — exact definition

Pillow must define one **canonical subject string** per sealed process proposal.
Recommended definition for Grok-compatible glob/prefix behavior:

```text
subject = program + (" " + argv_i for each argv entry in order)
```

Examples:

| Sealed proposal | Subject string |
| --- | --- |
| `program=cargo`, `argv=["test","-p","kernel-policy"]` | `cargo test -p kernel-policy` |
| `program=cargo`, `argv=["--version"]` | `cargo --version` |
| `program=node`, `argv=[]` | `cargo` wait: `node` only → `node` |

Matching rules (Pillow design, **modeled on** public Grok Bash reference):

1. **Bare `Bash` / bare process class rule**: matches every process proposal.
2. **Pattern match**: a rule hits if either:
   - **prefix**: `subject` starts with the pattern text (character-for-character), or
   - **glob**: pattern matches the **entire** `subject` with `*` / `**` semantics
     (engine details: implement against a fixed, tested matrix; see 未确定).
3. **No shell splitting** in C4.0: Pillow does not accept `a && b` as one
   process subject. Multi-command chaining is out of scope until a shell
   capability exists (explicitly forbidden today).
4. **Catalog still binds**: even with `Bash(cargo*)` allow, program must be a
   catalog-allowed label. Rules cannot introduce `pnpm` / `git` / shells.

**Critical Q&A for implementers:**

| Question | Answer |
| --- | --- |
| Does `Bash(cargo*)` match `program=cargo`, `argv=["--version"]`? | **Yes**, subject `cargo --version` prefix/glob-matches `cargo*`. |
| Does `Bash(cargo)` match `cargo test`? | **Yes** under prefix semantics (same as public Grok: no word boundary). Prefer documenting `Bash(cargo *)` for “program cargo only”. |
| Does matching use native executable path? | **No.** Only catalog `program` + argv subject. |
| Does matching use shell-quoted join? | **No.** Space-joined literal argv tokens only; no re-quoting. |
| Is `argv_digest` used for rules? | **No** for pattern match; digest remains for integrity/CAS. |

未确定 carried from Grok: exact glob library behavior (`**` vs `*`, case
folding on Windows). Pillow must pin a **documented, tested** algorithm and
treat Grok interop as “same user-facing patterns,” not bit-identical engines.

### 5.3 Write / Edit path subject

| Field | Rule |
| --- | --- |
| Subject | Portable relative path exactly as validated on the proposal (e.g. `src/lib.rs`) |
| Glob | `*` and `**` path globs relative to project root |
| Outside root | Already impossible via C3 path grammar; deny rules for `/etc/**` are redundant but harmless if path never leaves project |
| Mode split | `Write` ↔ create; `Edit` ↔ modify. A single rule class `FsWrite(...)` may be offered as Pillow alias covering both modes for UX |

### 5.4 Read / Grep

Present as mapping rows for future symmetry. **Minimal slice does not need them**
to fix “click ten times,” because those tools do not drive the approval bar
today. If deny-on-read is desired later (e.g. block reading secrets paths
beyond structural denylist), use the same portable-path subject.

### 5.5 Capability gaps (no design)

| Gap | Action |
| --- | --- |
| `WebFetch` | Do not invent a Pillow tool. If network tools appear later, design match subject then (URL vs `domain:host`). |
| `MCPTool` | Do not invent MCP. If MCP lands, match subject should be the tool name string used in the host registry, not free provider text. |

---

## 6. Storage and scope

### 6.1 Recommendation: dedicated rules table (not free-form settings blob)

**Adopt a first-class SQLite table** (name suggestion: `permission_rules`),
project-scoped governance object, rather than stuffing a JSON array into
`settings_entries`.

Reasons:

- Rules need stable **ids**, ordering, enable flags, and content snapshots for
  receipts (settings key JSON is awkward for per-rule attribution).
- ADR-0003 S0 known-key allowlist is intentionally narrow; permission rules are
  a **new domain object**, closer to proposals than to theme prefs.
- Audit queries (“which rules auto-approved writes last week?”) need rows.

Settings UI may still **surface** rules next to S0 settings; storage authority
is the rules table.

### 6.2 Suggested row shape (design, not schema lock)

| Column | Purpose |
| --- | --- |
| `rule_id` | UUID |
| `scope` | `global` \| `project` |
| `project_id` | NULL for global; required for project |
| `action` | `allow` \| `deny` (later: `ask`) |
| `prefix` | `process` \| `write` \| `edit` \| `read` \| `search` (or store Grok-facing string + normalized enum) |
| `pattern` | Glob/prefix body; empty means bare prefix / whole class |
| `pattern_canonical` | Normalized form used for matching (locked algorithm version) |
| `enabled` | bool |
| `priority` | Integer order within same action (optional; deny-all still wins by action class) |
| `created_at` / `updated_at` / `updated_by` | Audit |
| `content_digest` | `sha256:` of canonical serialized rule body for receipt binding |

**Hard write validators (host):**

- Global: `action` may be `allow` or `deny`.
- Project: **`action` must be `deny` only** (tighten). Reject project `allow` at
  insert/update (fail closed).
- Never load rules from files under the project tree.
- Never accept renderer-supplied “effective rule set” for evaluation; host
  loads from SQLite only.
- Cap row count and pattern length (bounds TBD in implementation plan).

### 6.3 Effective rule set resolution (tighten-only)

```text
effective = global_rules ∪ project_deny_rules(project_id)

evaluation order for a subject:
  1. any matching deny  → Deny
  2. any matching allow → Allow   (allows exist only from global)
  3. else               → Ask (human ApproveOnce / Reject)
```

This matches controller §3.2–3.3 even though Grok itself may merge project
allows from `.grok/config.toml`. **Pillow intentionally diverges** for trust.

Optional later: global `ask` rules that force a prompt even when an allow would
hit (Grok’s deny > ask > allow). Not required for minimal slice.

### 6.4 Relationship to S0 settings

| Concern | Authority |
| --- | --- |
| Theme, runtime adapter paths | `settings_entries` (S0) |
| Permission rules | `permission_rules` (this design) |
| “Rules UI lives under Settings” | Presentation only |
| Env override of rules | **Default: no.** Rules are safety-sensitive; env-driven allow would be hard to audit. If ever needed, allow only a deny-forcing kill switch, not allow injection. |

### 6.5 Explicit non-goal: project directory config

| Forbidden source | Why |
| --- | --- |
| `<project>/.pillow/permissions.toml` | Opening a repo would self-authorize |
| `<project>/.grok/config.toml` permission section | Same class of attack; Grok may use it, Pillow must not for **allow** |
| Provider tool arguments that embed rules | Provider must not author policy |

Project **deny** rules may only be written through Pillow’s host settings UI /
host API into SQLite, scoped by `project_id` the host already trusts.

---

## 7. Receipts and audit

### 7.1 Question receipts must answer

After the fact:

1. **Who authorized this execution?** Human click vs host rule engine.
2. **Which rule?** Stable `rule_id`.
3. **What was the rule text at decision time?** Snapshot or content digest +
   immutable snapshot columns (rules can be edited later).
4. **What was matched?** Subject string / path + proposal fingerprint.
5. **Was it allow or deny?**

### 7.2 Decision row extension (design)

Extend `write_approval_decisions` and `exec_approval_decisions` (or parallel
columns) beyond `approve_once` | `reject`:

| Field | Human ApproveOnce | Rule allow | Rule deny |
| --- | --- | --- | --- |
| `decision` | `approve_once` | `approve_by_rule` | `reject_by_rule` |
| `actor` | `user` | `rule_engine` | `rule_engine` |
| `approval_grant_digest` | existing | synthetic host grant digest (still unique) | may be N/A if reject has no grant today — keep parity with current reject rows |
| `proposal_fingerprint` | existing | existing | existing |
| `rule_id` | NULL | required | required |
| `rule_scope` | NULL | `global` / `project` | `global` / `project` |
| `rule_action` | NULL | `allow` | `deny` |
| `rule_prefix` | NULL | e.g. `process` | e.g. `write` |
| `rule_pattern` | NULL | snapshot text | snapshot text |
| `rule_content_digest` | NULL | `sha256:…` | `sha256:…` |
| `match_subject` | NULL | subject string used | subject string used |
| `decided_at` | existing | existing | existing |

**Do not** store raw approval grants. **Do not** store file contents.

### 7.3 Timeline / run events

Emit existing decided events with an extra bounded summary field, e.g.:

- `decision_source: user | rule`
- `rule_summary: "allow process:cargo *"` (bounded, path-free, no secrets)

Renderer must be able to show **“规则自动放行”** / **“规则拒绝”** without
opening Settings.

### 7.4 Grant path under auto-approve

Design intent:

1. Rule engine runs **inside the host** after durable proposal create (or
   immediately before presenting the bar).
2. On allow: host writes decision row with rule snapshot, mints grant (or
   uses host-only apply token), transitions to `Approved`, proceeds to apply /
   spawn **without** renderer `approve_once` command.
3. On deny: host writes reject decision with rule snapshot; no spawn/apply.
4. Renderer never supplies “I match rule X — skip grant.”

This preserves C3/C4 “renderer is not authority.”

---

## 8. UI morphology (design only)

### 8.1 Where users manage rules

| Surface | Content |
| --- | --- |
| Settings → **Permission rules** (global) | List allow + deny; add/edit/disable/delete; pattern helper |
| Project settings → **Extra denies** | Deny-only list for this project; explain cannot allow |
| Empty state | Short Grok-shaped examples: `process: cargo test *`, `edit: src/**/*.rs` |

Prefer product language that maps to Grok prefixes for users coming from Grok,
while showing Pillow tool names underneath (`process.run`, `fs.write`).

### 8.2 How users add a rule from a live prompt (high value)

On a pending approval bar:

- **Approve once** (today)
- **Reject** (today)
- **Always allow matching…** (creates a **global allow** with a **narrow**
  suggested pattern derived from the subject — e.g. `cargo test *` not bare
  `process`)
- **Never allow matching…** (creates deny at global or project — default
  project deny if a project is active)

“Always allow” must open a **confirm dialog** showing the exact pattern and
scope, not silently write a bare `Bash` allow.

### 8.3 Visibility of auto-approval (non-negotiable)

| Moment | UI requirement |
| --- | --- |
| During turn | Timeline row: “已由规则自动批准” + rule summary chip |
| Process/write activity row | Distinct visual from human-approved (icon/color/label) |
| Approval bar | Does **not** flash and vanish without history; if auto-path wins, bar never appears, but timeline still records |
| Settings | “Recently used by auto-approve” optional later |
| Audit/export later | Decision rows queryable |

**Forbidden UX:** silent auto-apply with no timeline mark (trains users to miss
agent side effects).

### 8.4 What not to build in UI v1

- Full Grok always-approve / YOLO mode as a single global kill-switch without
  rules (too coarse; undermines audit narrative). If ever added, it must still
  write per-execution receipts with `decision=approve_by_mode` and remain
  deny-bound.
- Editing rules from the project tree.
- Importing `.grok/config.toml` allow lists blindly.

---

## 9. Evaluation placement in the host

Suggested hook points (names illustrative):

```text
create_write_proposal / create_exec_proposal
  → persist PendingApproval
  → load effective rules (global + project deny)
  → evaluate(subject)
  → Deny: apply_reject_by_rule + receipt fields
  → Allow: apply_approve_by_rule + grant + continue apply/spawn path
  → Ask: expose decisionAvailable to UI (today)
```

Policy pure functions can live in `kernel-policy` (new module
`permission_rules.rs`) with no I/O; storage and grant minting stay in host /
tool-executor / runtime-host.

Structural policies (catalog, path denylist) run **before** or **with** rule
evaluation and always win as hard errors (no proposal or invalid proposal).

---

## 10. Phased delivery and minimal slice

### 10.1 Minimal slice (solves “click ten times”)

**Goal:** user can stop approving every `cargo test` and every edit under
`src/**` while keeping full receipts.

| Item | Include |
| --- | --- |
| Rule actions | `allow`, `deny` only |
| Prefixes | `process` (Bash), `write`, `edit` |
| Storage | `permission_rules` table + host CRUD |
| Scope | Global allow/deny; project deny-only |
| Evaluation | On write + process pending proposals |
| Receipts | `approve_by_rule` / `reject_by_rule` + rule snapshot fields |
| UI | Settings list + “always allow this pattern” from approval bar + timeline auto-approve label |
| Tests | Pure match matrix; project cannot insert allow; deny > allow; receipt fields present; no project-file load |

**Out of minimal slice:** Read/Grep rules, `ask` action, remembered interactive
grants, always-approve mode, sandbox, MCP/WebFetch, shell-chain semantics,
import from Grok config files.

### 10.2 Slice 2 (quality of life)

- `ask` action (force prompt)
- Suggested patterns library (“Rust workspace”, “deny `.env*` writes” — note
  structural deny may already cover some)
- Project deny templates
- Rule hit counters (non-secret telemetry in SQLite)

### 10.3 Slice 3 (optional / later)

- Read/search deny rules if product wants extra confidentiality beyond path
  policy
- Capability-gated network tool rules when such tools exist
- OS sandbox profile as layer D (independent design)
- Multi-role default rule packs (see §11)

### 10.4 Explicit non-goals for all slices here

- Replacing C3/C4 grants
- Renderer-authored effective policy
- Softening process catalog via allow rules
- Git mutation via rules

---

## 11. Observation: Grok `bundled/roles` shape (for later multi-role dispatch)

Local Grok bundle (manifest version observed: `public-2026-08-08-r2`) separates:

| Artifact | Shape | Examples |
| --- | --- | --- |
| **roles** (`bundled/roles/*.toml`) | Small TOML: `description`, `default_capability_mode`, optional `reasoning_effort`, `default_fork_context` | `explore` / `plan` / `quick-search` → `read-only`; `implementer` / `reviewer` / `test-writer` / `security-auditor` / design-doc roles → `all` or high reasoning |
| **agents** (`bundled/agents/*.md`) | Markdown with frontmatter: `name`, `description`, `prompt_mode`, `permission_mode`, `agents_md`, body instructions | `explore.md` / `plan.md` set `permission_mode: plan` and emphasize read-only tool use |
| **personas** (`bundled/personas/*.toml`) | Tone/instructions + optional `[[inputs]]` / `[[outputs]]` file contracts + capability defaults | `implementer.toml` wires review_file / summary_file |

**Implications for Pillow (observation only, not this slice):**

- Grok “role” is primarily **capability mode + model effort**, not a full
  permission-rule pack.
- Read-only roles still **rely on** tool presence / permission_mode / prompts;
  they are not identical to Pillow’s SQLite allow/deny tables.
- A future Pillow multi-role dispatcher could attach **default deny packs**
  (e.g. explore role ⇒ deny all `write`/`edit`/`process` allows) **as host
  policy overlays**, still global-user-authored or product-bundled, never
  project-tree authored.
- Do not conflate Grok `permission_mode: plan` with Pillow rules; plan mode is
  a separate Grok product surface.

---

## 12. Sandbox note (layer D, not this design)

| | Permission rules (A) | OS sandbox (D) |
| --- | --- | --- |
| Mechanism | Policy evaluation before execute | Kernel FS/network confinement |
| Failure mode | Prompt or deny | OS denies syscall / spawn |
| Grok default | Rules optional; modes optional | Sandbox **off** by default |
| Pillow today | Missing (this design) | Missing |
| Worth later? | **Yes — high priority for usability** | **Maybe later** for untrusted repos / defense in depth on Windows (platform support differs; Grok documents Linux/macOS primitives) |

Implementing A does not require D. D does not remove the need for A’s
receipts.

---

## 13. Residual risks and open items

| ID | Risk / open item | Severity | Mitigation |
| --- | --- | --- | --- |
| R1 | Over-broad allow (`process` bare / `cargo*`) auto-runs dangerous argv | High | UI suggests narrow patterns; default examples never bare `process`; consider future argv denylist overlays |
| R2 | Prefix match without word boundary (`cargo` matches `cargo-foo` if such a label existed) | Medium | Catalog is closed; document Grok-compatible behavior; prefer `cargo *` |
| R3 | Project allow smuggled via settings JSON bug | Critical | Schema + host validator: project action ∈ {deny} only; tests |
| R4 | Auto-approve without timeline visibility | High | Product acceptance: timeline label mandatory |
| R5 | Rule edited after decision → audit confusion | Medium | Snapshot pattern + content_digest on decision row |
| R6 | Glob engine 未确定 vs Grok bit-identical | Medium | Pin Pillow algorithm + golden tests; document “Grok-shaped” not “Grok-identical” |
| R7 | Synthetic grant differs from human grant paths | Medium | Shared apply/spawn codepath; only decision source differs |
| R8 | Users expect project `.grok` allows (Grok habit) | Low | Explicit UX copy: allows are user-global only |
| R9 | Read tools not gated — users may think rules cover reads | Low | Docs + UI scope labels |
| R10 | Concurrent rule edit during evaluate | Low | Read rules in same transaction as decision write |
| U1 | Exact Windows path glob case folding | 未确定 | Decide in implementation ADR |
| U2 | Whether Grok allow on chained shell is whole-string only | 未确定 | Irrelevant for C4 (no shell chains) |
| U3 | Public guide `ask` + remembered grants full persistence format | 未确定 | Out of minimal slice |

---

## 14. Worked examples

### 14.1 Development day (minimal slice)

Global rules:

```text
allow  process  cargo test *
allow  process  cargo check *
allow  process  cargo clippy *
allow  edit     src/**
allow  write    src/**
deny   process  cargo *--*release*    # example tighten; pattern skill dependent
```

Project deny:

```text
deny   edit     secrets/**
deny   write    secrets/**
```

Effects:

- `cargo test -p kernel-policy` → auto-approve + receipt `approve_by_rule`.
- `fs.write` modify `src/lib.rs` → auto-approve.
- `fs.write` create `README.md` → **ask** (no allow).
- `fs.write` modify `secrets/x` → **deny** (project) even if a mistaken global
  allow existed for `**` (deny wins).

### 14.2 Malicious project (must fail)

Project tries to store allow `process *` → **host rejects write**.  
Repo contains `.pillow/allow-all.toml` → **never read**.

---

## 15. Acceptance criteria for a future implementation plan

A later implementation is acceptable only if:

1. No project-scoped allow can be stored or evaluated.
2. Deny always wins over allow.
3. No match ⇒ human confirmation (unchanged default).
4. Every auto-approved write/process has decision row fields in §7.2.
5. Timeline/UI marks auto-approval visibly.
6. Process subject definition is the space-joined `program` + `argv` string;
   golden tests cover `cargo` + `["--version"]` vs patterns.
7. Structural catalog/path policy still fails closed independently of rules.
8. No reads of project-tree config for authorization.
9. Gates required by repo policy pass when implementation is authorized
   (this design task does not run them).

---

## 16. Document control

| Item | Value |
| --- | --- |
| Output path | `docs/superpowers/specs/2026-08-10-permission-rules-design.md` |
| Code changes | **None** |
| Builds / tests run | **None** (controller forbid) |
| Auth files read | **None** |

---

## 17. Completion

**PERMISSION-DESIGN-COMPLETE**
