# Pre-audit consistency sweep report

- Date: 2026-08-12
- Worker: grok4
- Controller: Claude
- Scope: read-only code verification of architecture claims; edit only outdated
  statements in `docs/handoff/**`, `README.md`, `AGENTS.md`, and this report
- Status: **COMPLETE**
- Signal: **PREAUDIT-SWEEP-COMPLETE**

## 0. Constraints (as executed)

- Allowed writes: `docs/handoff/**`, `README.md`, `AGENTS.md`, this report
- Forbidden: any product/code changes; `docs/references/external-audit-brief.md`
- No git write operations
- Docs must not contain machine absolute paths, keys/tokens, lease/session IDs,
  or maintainer-only `.superpowers/**` operational detail

Focus verification (high-churn items):

1. Slot model: one record, `auth` + `role`, governance derived from auth
2. Roles: Controller / Librarian / Custom; Controller unique + governed
3. Execution paths: GrokShell / NativeAdapter / NotRunnable
4. Auth: ApiKey / GrokLogin / LocalCli / Oauth
5. Crates: `provider-adapters`, `role-dispatch`, `cli-detect`
6. Dispatch = ordinary governed process proposal (not a second mechanism)

---

## 1. Exit code table (actually run)

| Command | EXIT | Notes |
|---------|------|-------|
| `node scripts/check-secrets.mjs > artifacts/preaudit-secrets.log 2>&1; echo EXIT=$LASTEXITCODE` | **0** | Closeout required; PowerShell `$LASTEXITCODE` |

No product build/test gates were required by this ticket (docs-only). Workspace
HEAD observed read-only: `ce017a5` on `main` (not written).

---

## 2. T1 — Outdated statements (full list)

### 2.1 Fixed in this ticket (allowed write surface)

| # | File:line (pre-fix) | Document said | Code actually is | Evidence |
|---|----------------------|---------------|------------------|----------|
| A1 | `docs/handoff/2026-08-11-pillow-controller-handoff.md:68` | Slots are two kinds: `governed_grok` (governed) / `external_cli` (register-only, ungoverned), with governance as an explicit model attribute | **One** slot record: `auth` + `role`. Governance is **derived** from auth and is not independently stored. Wire labels `governed_grok`/`external_cli` remain **derived** for renderer compatibility only | `crates/storage-sqlite/src/runtime_slot.rs:1-4`, `:89-125`, `:99-105`; migration `0017_uniform_slot_model.sql` (drops independent governance column) |
| A2 | Same handoff §1 / §3 | Auth/path model implied only login + API key + external CLI; no roles, no multi-path execution, no OAuth, no dispatch, no new crates | Auth: `ApiKey`/`GrokLogin`/`LocalCli`/`Oauth`. Roles: `Controller`/`Librarian`/`Custom`. Paths: `GrokShell`/`NativeAdapter`/`NotRunnable`. Dispatch reuses process governance. Workspace members include the three crates | `runtime_slot.rs:52-87`, `:130-137`; `execution_path.rs:19-26`, `:143-189`; `Cargo.toml:7-18`; `role-dispatch/src/lib.rs:1-15` |
| A3 | Same handoff header + §7 + §11 | Contained machine absolute paths, desktop key-file location, and maintainer ledger path under private ops tree | Must not ship those in public handoff prose | Scrubbed in T2 (see §3) |
| A4 | `README.md:42-44` | “C4 is checkpointed… Interactive shell and Git mutation have not been started.” | C4 checkpoint text was true **on 2026-08-07**. Since then: governed shell + Git mutation **backends** landed; slots/OAuth/dispatch continued | `docs/execution/2026-08-11-governed-shell-report.md`; `apps/desktop/src-tauri/src/git_commands.rs` stage/unstage/commit/push; adapter profile tools include `run_terminal_cmd` (`methods.rs`); handoff §3 tools table |
| A5 | `AGENTS.md:58-60` | Pointed maintainers at private runbook path; product “slots” vocabulary could be confused with maintainer worker farms | Product runtime slots are project-scoped domain objects; maintainer farms are **not** product contract | Clarified in T2; domain evidence: `runtime_slot.rs` header + `create_slot` in `runtime-slots/src/lib.rs:339-346` |

### 2.2 Still outdated — **not fixed** (outside write boundary or uncertain)

| # | File:line | Document says | Code actually is | Evidence | Disposition |
|---|-----------|---------------|------------------|----------|--------------|
| B1 | `docs/roadmap/2026-07-31-stage1-overall-status.md:85` | “Interactive shell and Git mutation remain unstarted” (C4 row) | Shell + Git mutation backends exist post-checkpoint | Same as A4 | **Controller:** update roadmap or stamp “frozen as of 08-07, superseded” |
| B2 | Same roadmap `:64-67`, `:86` | “Only Grok Build speaks ACP… built-in agent shell is Grok Build”; multi-slot = API-key/login only | Path matrix includes **NativeAdapter**; auth includes **Oauth**; CLI detect/registry exist | `execution_path.rs:143-216`; migration `0018_oauth_slots.sql` | **Controller:** refresh Stage 1 ladder “D multi-slot” row |
| B3 | `docs/verification/2026-08-07-wave-c4-checkpoint.md:35-41` | Explicitly out of scope: interactive shell, Git mutation, credential/key management, additional runtimes | Historical **checkpoint** (correct for that day); product has moved | Checkpoint date header; later execution reports | **Do not rewrite history.** Point readers to latest handoff/roadmap |
| B4 | `docs/execution/2026-08-11-slots-foundation-report.md` (throughout) | Two SQL kinds `governed_grok`+`full` / `external_cli`+`none` as stored shape | Superseded by uniform model (0017) + OAuth (0018) | Migrations 0015→0017→0018 | Historical worker report — leave; truth is code + latest handoff |
| B5 | `docs/superpowers/plans/2026-08-11-slots-foundation.md` | Foundation plan for pre-uniform model | Plan-as-written; many steps done under later tickets | Plan file vs current schema | Historical plan — leave |
| B6 | `docs/research/2026-08-12-slot-auth-breadth.md:73` | Auth→governance mentions only ApiKey/GrokLogin/LocalCli (snapshot `570828c`) | OAuth maps to Full; path may be NativeAdapter | `governance_from_auth` + `resolve_execution_path` Oauth arm | Research snapshot; note date, do not silently “update” unless controller wants |
| B7 | Older handoffs `2026-08-01` / `08-07` / `08-08` / `08-10` | Pre-boundary-rebuild or pre-slot truth | Explicitly superseded by later handoffs | Each file’s supersession banner | Leave as archive |

### 2.3 Claims verified as **still true** (no edit)

| Claim | Evidence |
|-------|----------|
| Write/process hands via native tools → host governance chain | Handoff §3 tools table matches `adapter-grok-acp` profile; governed write + process still host-owned |
| `bypass` turns off asking, not governance | Handoff §3; permission-mode design + tests (not re-litigated here) |
| Half-open traps (LocalFs / LocalTerminal / ambient MCP) | Coupling tests in `methods.rs` still the right story |
| Host DTO closed key allowlist is a breaking-change surface | `AGENTS.md` engineering rule still matches gateway design |
| Domain state in SQLite; desktop shell must not own agent-domain logic | Architecture still holds |
| Dispatch reuses process proposal chain (no second approver) | `role-dispatch/src/lib.rs:7-15`; `conversation_service::propose_role_dispatch` → `prepare_dispatch` / `decide_exec_proposal` path |
| Controller unique + must be governed | `idx_runtime_slots_one_controller`; `ControllerRequiresGovernedAuth` |
| New crates present in workspace | Root `Cargo.toml` members: `cli-detect`, `provider-adapters`, `role-dispatch` |

---

## 3. T2 — Fixes applied

| File | Change | Why |
|------|--------|-----|
| `docs/handoff/2026-08-11-pillow-controller-handoff.md` | Rewrote §3 slot paragraph to uniform `auth`+`role`, four auths, three roles, three execution paths, wire-label caveat, three crates, dispatch = process proposal; flagged NativeAdapter “prepared only” | Direct contradiction with storage + runtime-slots + host start path (A1/A2) |
| Same | Scrubbed work-dir absolute path, tryout absolute path, desktop key-file location, private ledger path; softened runbook pointer | Public-doc hygiene rules |
| Same | Header note that 2026-08-12 pre-audit patch applied; kept §8 known gaps intact | Traceability without deleting adverse content |
| `README.md` | Updated Stage 1 status: shell/Git backends + slots; C4 checkpoint framed as historical; pointer to latest handoff | A4 |
| `AGENTS.md` | Clarified maintainer farms ≠ product runtime slots; dropped private runbook path | A5 + hygiene |

**Not changed (on purpose):**

- Known defects / residual risks in handoff §8
- Historical execution reports, ADRs, research snapshots, checkpoint records
- `docs/references/external-audit-brief.md` (controller-owned)
- Any code

**Uncertain → left for controller (not “fixed for beauty”):**

- Whether Git **mutation UI** is product-complete enough to claim “Git mutation shipped” in marketing language (backend + Tauri commands yes; full renderer story not re-audited here)
- Exact HEAD/commit counts in handoff narrative (stale numbers removed rather than re-counted)

---

## 4. T3 — Questions auditors will ask that docs did not answer

This is the highest-value section. None of the following required inventing new
product claims; they are gaps between **public prose** and **what the tree
actually proves**.

### 4.1 NativeAdapter is a path, not a live conversation runtime (yet)

- **What code does:** `start_runtime_slot` for `NativeAdapter` runs `prepare_slot`,
  returns `status: "native_adapter_prepared"`, and **does not** `activate_grok`
  (`apps/desktop/src-tauri/src/runtime_slot_commands.rs` ~1034–1063). Focused
  desktop test pins “no active slot” for chatgpt-codex OAuth
  (`runtime_slot_commands_tests.rs` `start_native_adapter_codex_slot_prepares_without_grok_shell`).
- **What docs often imply:** “execution path” language can be read as “user can
  chat on that path.”
- **Auditor question:** *Can a NativeAdapter slot complete a real multi-turn
  conversation with tools/write/process under host governance, or only prepare
  an HTTP request package?*
- **Evidence class today:** unit/integration prepare tests + host refuse-to-stuff;
  **not** a live end-to-end conversation acceptance for NativeAdapter.

### 4.2 Full governance for NativeAdapter is a mechanism claim, not a live probe

- Mechanism constant: `NATIVE_ADAPTER_GOVERNANCE_MECHANISM =
  "host_harness_tools_write_process"` (`execution_path.rs`).
- Comment asserts tools/write/process stay on host because Pillow **is** the
  harness.
- **Auditor question:** *Where is the live (or even host-integration) proof that
  a NativeAdapter turn cannot open a half-open trap analogous to LocalFs /
  LocalTerminal / ambient MCP?* GrokShell has MCP probe scripts and coupling
  tests; NativeAdapter’s “Full” is largely **by construction** until a harness
  exists.

### 4.3 OAuth slots cannot be role-dispatch targets

- Storage/runtime support `SlotAuth::Oauth`.
- `role-dispatch` `SlotAuthShape` has only ApiKey / GrokLogin / LocalCli
  (`role-dispatch/src/types.rs`).
- Host mapper **fail-closes** on OAuth:
  `role_dispatch_unsupported_auth`
  (`role_dispatch_commands.rs` `map_slot_auth`, ~188–212).
- **Auditor question:** *If Controller is an OAuth slot, or a worker is OAuth,
  how does dispatch work?* Answer today: it does not map.

### 4.4 Wire vocabulary lies for compatibility

- Domain: four `auth_kind`s + three `role_kind`s.
- Wire DTO still emits `kind: "governed_grok"` for ApiKey **and** GrokLogin
  **and** Oauth, including NativeAdapter starts
  (`SlotAuth::wire_kind`, start DTO).
- **Auditor question:** *Is `governed_grok` a product type or a legacy label?*
  Docs that still teach two kinds will waste audit time; even fixed handoff must
  be cross-checked against UI strings.

### 4.5 Consumer-subscription OAuth vs standard OIDC

- Storage comment: consumer ChatGPT Plus / Claude Pro backends “intentionally
  unsupported”; only standard OIDC that yields a bearer for OpenAI-compatible
  base_url (`runtime_slot.rs:48-50`).
- Simultaneously: `chatgpt-codex` is a first-class provider id that **forces**
  NativeAdapter (`provider-adapters` + `resolve_execution_path`).
- **Auditor question:** *Is chatgpt-codex a supported product path or a
  structural placeholder?* Need a single public sentence.

### 4.6 Dispatch durability / audit surface

- `role-dispatch` crate: pure types; “storage is a later controller wiring step”
  in crate docs.
- Host wires propose → governed process reserve; `list_role_dispatches` is
  **in-memory for this host instance** (command docs on
  `list_role_dispatches`).
- **Auditor question:** *After restart, where is the dispatch audit trail?*
  Process receipts may exist; assignment ledger durability is easy to over-claim.

### 4.7 LocalCli “governance none” vs “incomplete path”

- Storage: `RuntimeSlotGovernance::None` for LocalCli.
- Path layer: `PathGovernance::Incomplete { LocalCliUngoverned }` /
  `NotRunnable`.
- Role-dispatch: `GovernanceCapability::None` and forbids elevation.
- **Auditor question:** *Are “none” and “incomplete” the same security claim?*
  Docs should use one vocabulary or explain the mapping.

### 4.8 Git mutation: backend vs product surface

- Backend + Tauri mutation commands exist (stage/unstage/commit/push) with hard
  path/protocol policy tests.
- 2026-08-11 mutation report itself said renderer was out of scope.
- Checkpoint docs still say mutation unstarted (historical).
- **Auditor question:** *What can a user actually do from the shipped UI today?*
  Need a single capability matrix (read Git / mutate Git / shell) with
  UI-vs-backend columns.

### 4.9 Security properties that are convention or single-path tests

Examples auditors will probe:

| Property | Stronger evidence | Weaker / convention |
|----------|-------------------|---------------------|
| Native tool allowlist fail-closed | Profile constants + denylist third-name test | Upstream silent profile fallback was the 08-11 incident — probe is the real authority |
| DPAPI secrets | Windows tests; non-Windows fail-closed codes | No cross-platform secret story for release |
| Shell segmentation / no whole-string allow | Policy tests | Interpreter allowlist finiteness admitted in handoff §8 |
| Bypass ≠ skip governance | Mode design + tests | Product UX copy may still say “auto-approve” in places (not re-scanned) |
| Renderer closed DTO keys | Gateway allowlists + AGENTS rule | Easy to ship host field without allowlist (historical thrice) |

### 4.10 “User has never run a real task”

- Handoff still correctly flags this as the largest unknown.
- **Auditor question:** *Which gates substitute for user acceptance?* E2E 10/10
  is not “a human drove a product task.” Do not bury this to look healthier.

### 4.11 Secret / path hygiene still present in older public docs

- This ticket scrubbed the **current** handoff.
- Older handoffs and some research/execution files may still contain absolute
  paths or private-layout hints (outside write boundary here).
- **Auditor question:** *What is the public tree publish filter?*
  `check-secrets.mjs` is necessary but not a full publishability review.

### 4.12 cli-detect / librarian surface depth

- Crates and host commands exist (`cli-detect`, librarian audit commands,
  execution reports dated 2026-08-12).
- Public handoff (even after fix) only **names** them.
- **Auditor question:** *Is Librarian a real product role with duties, or a
  role enum + audit helpers?* Point to concrete UX + durable checks vs
  aspirational ADR-0004 language.

---

## 5. Write-boundary audit

| Path | Action | Allowed? |
|------|--------|----------|
| `docs/execution/2026-08-12-preaudit-sweep-report.md` | create/update | yes (this report) |
| `docs/handoff/2026-08-11-pillow-controller-handoff.md` | edit (architecture + hygiene) | yes |
| `README.md` | edit (status) | yes |
| `AGENTS.md` | edit (slot vocabulary clarification) | yes |
| Any `crates/**`, `apps/**`, `scripts/**` | **none** | n/a |
| `docs/references/external-audit-brief.md` | **none** | forbidden by ticket |
| git commit/push/add | **none** | forbidden by ticket |

---

## 6. Focus-item verdict (for controller)

| Focus item | Doc-vs-code before sweep | After T2 |
|------------|--------------------------|----------|
| 1. One record, auth+role, governance from auth | Handoff wrong (two kinds) | Handoff corrected |
| 2. Controller / Librarian / Custom; Controller unique+governed | Missing from handoff | Present in handoff |
| 3. GrokShell / NativeAdapter / NotRunnable | Missing; implied Grok-only | Present + **prepared-only** caveat |
| 4. ApiKey / GrokLogin / LocalCli / Oauth | Partial (no Oauth) | Present |
| 5. provider-adapters / role-dispatch / cli-detect | Missing | Named |
| 6. Dispatch = ordinary process proposal | Missing | Present |

---

## Completion

- Signal: **PREAUDIT-SWEEP-COMPLETE**
- Secrets gate: **EXIT=0**
- Residual for controller: roadmap B1/B2, NativeAdapter product honesty (T3.1–T3.2),
  OAuth×dispatch gap (T3.3), wire-label story (T3.4), publish filter beyond this
  handoff (T3.11)
