# Pillow Settings Layer and Credentials Management Design

Date: 2026-08-08

Status: **Proposed design for controller acceptance; implementation not
authorized**

Owner: design worker (Grok); controller: Claude

Work directory: `<repo>`

## 1. Decision

Pillow adds a **settings and secret-reference** capability ladder so an
industrial product can run on someone else's machine without requiring
environment variables or out-of-band files as the only configuration path.

| Approach | Decision | Reason |
| --- | --- | --- |
| One design, two delivery rungs (S0 settings, S1 credentials) | **Adopt** | Shared host/SQLite surfaces; different security and approval shapes; avoids a second product slice that re-implements scope and migration |
| Settings-only slice, credentials deferred indefinitely | Reject as product strategy | Acceptable as **S0 shipping gate** (see §4.3), not as final industrial posture |
| Credentials store secrets in SQLite (encrypted blob or plaintext) | **Reject** | Violates pillow-first secret boundary and AGENTS.md; dump/backup/sync become secret stores |
| Renderer holds or displays secret values after entry | **Reject** | Violates Z4 least privilege and C3 host-private grant pattern |
| Pillow takes over Grok CLI auth.json / slot login | **Reject** | Slot containers and Keeper routing own Grok identity; Pillow must not open or rewrite CLI auth |

**Recommended product framing:** treat this as **one capability boundary**
documented here, delivered as **S0 then S1**, not as two unrelated features
that invent separate storage and UI systems.

## 2. Why Now

Current Stage-1 desktop composition:

- Runtime adapter selection is env-only (`PILLOW_RUNTIME` in
  `apps/desktop/src-tauri/src/runtime_composition.rs`).
- Grok executable and profile paths are env-only
  (`PILLOW_GROK_EXECUTABLE`, `PILLOW_GROK_PROFILE`).
- There is **no durable user settings model** in SQLite.
- Provider credentials live **outside** Pillow (Grok CLI session under process
  home). Pillow intentionally never opens auth material.

That is correct for development and for "operator already logged into Grok
CLI," but it is a hard block for an industrial product handed to another
person's machine, another project tree, and eventually another provider that
expects an API key or OAuth token.

This design stays in **Stage 1** (industrial local AI IDE). It does not pull
in Obsidian (Stage 2) or company operations (Stage 3).

## 3. Product Scope

### 3.1 Included (this design)

**S0 — Settings layer (non-secret configuration)**

- SQLite-authoritative settings rows (global and project scopes).
- Host-resolved effective settings for desktop composition and adapters.
- User-facing settings surface for non-secret preferences already implied by
  product code or accepted designs:
  - preferred runtime adapter label (e.g. `grok-acp`, explicit `fake` only for
    development builds/policy);
  - Grok executable path (path only);
  - Grok process-home / profile directory (path only; existence check only);
  - UI preferences that do not change security policy (theme, pane density,
    default resource-pane mode) if the product already has soft defaults;
  - feature toggles that **gate already-accepted C3/C4 capabilities** without
    reopening their authority (see §5.4).
- Env vars remain **override / bootstrap** inputs, not the durable authority
  after S0 lands (precedence in §5.3).
- Schema migration through existing `storage-sqlite` ordered migrations.
- Restart recovery of non-secret settings without human re-entry.

**S1 — Credential management (secret material)**

- OS-backed secret storage (see §6.2).
- SQLite stores **opaque secret references and metadata only**.
- Host-private entry path: secret bytes never re-cross the Tauri boundary to
  the renderer after submit (and never appear in logs, run events, or
  diagnostics).
- Lifecycle: enroll, rotate, revoke, delete; durable non-secret receipts.
- Connection records that bind a provider kind to a secret reference (or to
  "external CLI auth" for Grok).
- Explicit non-takeover of Grok CLI auth (§6.7).

### 3.2 Explicitly excluded

- Stage 2 Obsidian / vault knowledge integration.
- Stage 3 company-operation workflows.
- Reading, printing, copying, or rewriting Grok `auth.json`, OAuth cookies,
  or Keeper slot credential files.
- Scraping credentials from other applications or browsers.
- Provider- or agent-initiated credential capture ("agent asks user for API
  key mid-turn and stores it") in S0/S1 — human opens Settings / Connections.
- Injecting secrets into `process.run@1` environment maps (C4 forbids
  provider/renderer env; S1 does not relax that).
- Multi-user OS accounts sharing one Pillow secret vault without re-login
  (Windows DPAPI/user scope is single interactive user).
- Cloud sync of settings or secrets.
- A second approval framework parallel to C3/C4 proposal grants.
- Codex/OpenCode adapter implementation itself (may **consume** S0/S1 later
  under separate designs).
- Auto-approve, allow-for-session, or YOLO modes for secrets or writes.

## 4. Required Judgments

### 4.1 Same capability slice or separate?

**Recommendation: one design boundary, two delivery rungs (S0 then S1).**

| Option | Pros | Cons |
| --- | --- | --- |
| **A. One design, S0→S1 rungs (recommended)** | Shared scope model, migrations, host service, UI shell; credentials never get a second storage story; controller accepts security once | Implementation plan must gate S1 until S0 green |
| B. Fully separate designs/slices | Smaller first PR | Risk of settings JSON vs secret-ref mismatch; duplicate UI; delayed security review |
| C. One monolithic ship of settings+credentials | One user-visible release | Couples high-risk crypto/OS work to low-risk prefs; slows S0 |

**Why A:** AGENTS.md already couples "domain state in SQLite" with "secret
boundaries below the UI." Settings without secret **references** is incomplete
for multi-provider, but settings without secret **values** is a valid S0.
Documenting both together prevents S0 from painting into a corner (e.g.
putting API keys into setting values).

### 4.2 Minimum shippable set

**Recommendation: ship S0 as the first externally useful settings gate;
S1 is required before non-Grok providers that need Pillow-held secrets.**

**S0 minimum (external handoff with Grok-only runtime):**

1. SQLite `settings` tables + repository + migration.
2. Host `SettingsService` resolving effective values with documented
   precedence.
3. Desktop settings UI for: runtime selection (constrained enum), Grok
   executable path, Grok profile path (paths only).
4. `runtime_composition` reads **resolved** settings (with env override for
   automation/tests) instead of env-only authority.
5. Focused tests: migration, precedence, project override, restart restore,
   no secret fields in settings schema.
6. No secret values in renderer DTOs (vacuously true if S0 has none).

**S1 minimum (product can hold a provider secret without leaking it):**

1. OS secret backend behind a narrow `SecretStore` trait.
2. SQLite `secret_refs` + `connection` metadata (no secret bytes).
3. Host-private enroll/rotate/delete commands; renderer sees only mask/status.
4. Redaction tests (canaries never appear in SQLite, logs, invoke responses,
   diagnostics).
5. Audit receipts for enroll/rotate/revoke/delete without secret material.
6. At least one **connection kind** that uses a secret ref (e.g. generic
   `api_key` for a future adapter) **or** a documented "external CLI auth"
   connection kind for Grok that stores **no** secret.

S0 alone is enough to stop requiring `PILLOW_*` env for ordinary Grok users
who already authenticate via CLI. S1 is the gate for "Pillow stores *your*
provider key."

### 4.3 Can we deliver externally without credential management?

**Yes, with a constrained product claim — not as full industrial multi-provider.**

| Delivery mode | Credentials in Pillow? | Who can use it | Cost / risk |
| --- | --- | --- | --- |
| **Grok-CLI-auth only (recommended interim)** | No (S0 only) | Users who install Grok CLI and complete CLI login; Pillow points at executable + profile path | Cannot onboard pure API-key providers; support burden stays on CLI login; profile path misconfiguration remains a footgun |
| Env-injected secrets (not recommended) | No durable store | Operators who set machine env | Secrets in process env, parent dumps, crash reports; conflicts with child env allow-lists; not product-grade |
| Full S1 | Yes (OS store + refs) | Industrial multi-machine / multi-provider path | Implementation and security test cost |

**Recommendation:** accept **Grok-CLI-auth + S0 settings** as an interim
external beta claim, with explicit product copy: "Grok authentication is
managed by the Grok CLI; Pillow does not store that login." Do **not** claim
generic provider credential management until S1 acceptance.

### 4.4 Does this touch the checkpointed C4 boundary?

**Soft configuration only; no reopen of C4 authority.**

| Area | Touches C4? | Rule |
| --- | --- | --- |
| `process.run@1` proposal/approval/grant/exec | **No** | Contracts, policy, and executor remain as accepted |
| Child environment policy (sensitive name denylist; no provider env) | **No change** | S1 must **not** add a path for renderer-supplied env or free-form secret injection into process tools |
| Toolchain root / executable discovery for registry labels | **Optional config only** | If S0 later stores non-secret toolchain roots, C4 still resolves labels → absolute paths under host policy; provider still cannot pass paths |
| Git read G0 | **No** | Unrelated |
| Runtime selection for conversation service | **Adjacent, not C4** | Lives in desktop composition / adapter layer (pre-C4) |

If a future design wants "inject API key into approved network tool," that is
a **new** governed capability with its own approval object — not a silent
settings flag and not a C4 process.env backdoor.

**C3 boundary:** settings do not alter write proposal content, grants, or
CAS. Credential path policy already denies writing credential-like files;
S1 does not loosen that.

## 5. Settings Layer Design

### 5.1 Authority storage

**Hard requirement:** durable settings live in **SQLite** (`keeper.db` under
the existing app data directory), not ad hoc JSON files.

Rationale (aligned with AGENTS.md and pillow-first design):

- One operational authority already used for projects, conversations, write
  proposals, exec proposals.
- Same migration runner (`crates/storage-sqlite` ordered `000N_*.sql`).
- Transactional updates and foreign keys to `projects` for project scope.
- Backup/export tooling can treat one database; secret **values** stay out of
  it (S1).

**Not allowed as authority:**

- `settings.json`, `.pillow/config.json`, TOML next to the project (may exist
  later only as **import/export** formats, never as live authority).
- Renderer `localStorage` / IndexedDB for durable product settings.
- Environment variables as sole long-term store (env is override/bootstrap).

### 5.2 Scope model

```text
machine / installation
  └── global settings (scope = global, project_id NULL)
        └── project settings (scope = project, project_id = <id>)
              └── effective settings for an active project session
```

| Scope | Owns | Examples |
| --- | --- | --- |
| **Global** | Installation defaults | Default runtime adapter, default Grok executable path, UI chrome prefs |
| **Project** | Overrides for one `projects.id` | Project-specific Grok profile path, project preferred runtime (if allowed), project feature toggles |

**Conflict rule (recommended):**

1. Start from **built-in defaults** (code constants).
2. Apply **global** SQLite rows.
3. Apply **project** SQLite rows for the active project (override same keys).
4. Apply **process environment overrides** only for keys on an explicit
   allow-list used by automation and tests (`PILLOW_RUNTIME`,
   `PILLOW_GROK_EXECUTABLE`, `PILLOW_GROK_PROFILE` initially).
5. **Fail closed** on invalid values (same spirit as
   `RuntimeSelection::parse` — no silent Fake).

**Why project wins over global:** every durable agent object is
project-scoped; a project opened on a shared machine should not inherit
another project's runtime home. Global remains the default when a project key
is unset.

**Why env still wins for allow-listed keys:** CI, desktop E2E, and support
reproduction must pin runtime without mutating the user's DB. Document env as
**ephemeral override**, not as user-facing configuration.

**Keys that must not be project-overridable (recommended):**

- Anything that would weaken installation-wide security policy (e.g. a future
  "disable audit" flag — should not exist at all).
- Secret **values** (they are never settings keys).

### 5.3 Key namespace and value types

Settings values are **typed, non-secret** JSON-compatible scalars/enums stored
as constrained columns (not free-form blob dumping).

Recommended shape (illustrative; implementation may split columns):

```text
settings_entries
  setting_id        TEXT PK
  scope             TEXT CHECK IN ('global','project')
  project_id        TEXT NULL REFERENCES projects(id)
  key               TEXT NOT NULL   -- dotted namespace, e.g. runtime.adapter
  value_kind        TEXT NOT NULL   -- enum | path | bool | u32 | string
  value_json        TEXT NOT NULL   -- typed payload; validated by host
  updated_at        TEXT NOT NULL
  updated_by        TEXT NOT NULL   -- 'user' | 'system' | 'migration'
  UNIQUE(scope, project_id, key)
  CHECK (scope='global' AND project_id IS NULL
      OR scope='project' AND project_id IS NOT NULL)
```

**Namespaces (S0):**

| Key | Kind | Default | Notes |
| --- | --- | --- | --- |
| `runtime.adapter` | enum | `grok-acp` | Same parse rules as today; `fake` only when product policy allows (dev builds or explicit advanced) |
| `runtime.grok.executable` | path | unset → PATH discovery | Path only; never open as secret |
| `runtime.grok.profile` | path | unset → current-user home | Existence check only; never read auth files inside |
| `ui.*` | bool/enum | product defaults | Non-security chrome only |

**Forbidden value content (host validation):**

- Keys matching sensitive name patterns used by process env policy
  (`KEY`, `TOKEN`, `SECRET`, `PASSWORD`, `CREDENTIAL`, `AUTH`, provider
  names) **as setting keys or string values that look like secrets**.
- Secret-shaped strings (`sk-…`, `xai-…`, `Bearer …`, long high-entropy
  tokens) in `value_json` — reject at write time with opaque error.
- Absolute paths that are not valid UTF-8; UNC uncanny paths may be allowed
  for executable/profile **only after** the same path safety review used
  elsewhere (prefer user-picked paths via existing folder/file dialogs).

### 5.4 C3/C4 feature switches vs new capability

| Setting class | Relation to C3/C4 | S0 allowed? |
| --- | --- | --- |
| Runtime composition (adapter, grok paths) | Pre-C4 desktop composition | **Yes — primary S0** |
| UI layout prefs | No authority change | Yes |
| "Disable write proposals" / "disable process.run" | Soft **capability gate** over existing tools | Optional; must default **on** (tools enabled as today); cannot invent auto-approve |
| "Auto-approve writes/process" | Would break C3/C4 Ask-default | **Forbidden** as a setting |
| Toolchain roots for C4 registry | Configures host discovery only | Defer unless needed; separate small design note if introduced |
| Network / MCP / credential tools | New capabilities | **Out of S0/S1 settings** |

**Recommendation:** S0 only ships runtime + safe UI keys. Capability kill
switches for C3/C4 are **not** required for external Grok delivery; if added
later they are fail-closed gates, never auto-approve.

### 5.5 Do settings changes require C3-style approval?

**Recommendation: No for ordinary non-secret settings; Yes (receipt, not
proposal-grant) for credential lifecycle; never silent agent-driven mutation.**

| Change type | Human interaction | C3 proposal/grant? | Durable receipt? |
| --- | --- | --- | --- |
| User edits `runtime.adapter` / paths / UI prefs in Settings UI | Direct form submit in trusted settings surface | **No** | Optional audit row (`settings_changed`) with key + old/new non-secret values |
| Agent/provider proposes changing settings | Not in S0/S1 | n/a | Out of scope |
| Enroll / rotate / delete secret (S1) | Dedicated confirmation step (see §6.5, §7) | **Reuse grant pattern for destructive/secret ops only** | **Required** (metadata-only) |
| Import settings file | Explicit user import action | Confirm summary | Yes |

**Rationale:**

- C3 approval exists because a **provider** proposes a **filesystem mutation**
  with sealed content the human must review. Settings edits are
  **user-originated** in a first-party UI the human already controls.
- Forcing every theme toggle through `approve_once` would train users to
  click-through approvals and weaken the signal for agent writes/process.
- Secrets and irreversible credential deletion **do** warrant a one-shot
  confirmation grant because the consequence is account takeover or loss of
  access — that reuses the **grant/receipt machinery**, not a second policy
  framework (see §7).

### 5.6 Restart recovery and schema migration

**Migrations:**

- Add `0009_settings.sql` (number illustrative; next free after current
  `0008_*`) via existing `apply()` chain in `storage-sqlite`.
- Idempotent `CREATE TABLE IF NOT EXISTS` style consistent with prior
  migrations.
- Do **not** rewrite historical migrations.
- Value validation lives in Rust host code; SQL CHECKs enforce scope shape and
  non-null basics only.

**Restart:**

- Settings rows are durable; no in-memory grant required for non-secret keys.
- On startup, host loads SQLite → builds effective config → composes runtime
  (replacing pure env authority for user mode).
- Invalid stored enum values fail closed with a clear settings error code and
  **do not** fall back to Fake.

**Export/import (optional later rung, not S0):**

- Export emits non-secret settings JSON for support.
- Import is user-confirmed and revalidated.
- Secret refs export as **presence + label only**, never OS secret bytes.

**Machine move:**

- Copying `keeper.db` alone must **not** move secrets (S1 OS store is
  user/machine bound). UI shows "connection needs re-enroll" when ref is
  dangling.

## 6. Credentials Management Design

### 6.1 Security invariants (non-negotiable)

| # | Invariant | Enforcement |
| --- | --- | --- |
| I1 | Secret **values** never enter SQLite | Schema has no secret columns; repository types are metadata-only; tests insert canaries and assert absence |
| I2 | Secret values never enter logs, tracing, panic messages, or run_event payloads | `Debug`/`Display` redaction on secret wrappers; logging layer denylist; protocol public errors are code-only |
| I3 | Secret values never return across Tauri to the renderer after accept | Enroll command takes secret only on the **inbound** invoke payload; response DTO is `{ secretId, label, status, providerKind }` with **no** secret field; list/get never include values |
| I4 | Renderer never persists secrets | No localStorage; gateway strips any accidental field (defense in depth, like `stripApprovalGrant`) |
| I5 | Adapter/process child env does not inherit parent secrets wholesale | Existing allow-lists in `adapter-grok-acp` / `process-executor` remain; S1 injects secrets only through an explicit, audited host path when a **future** connection-scoped launch design requires it — **not** into C4 `process.run` |
| I6 | Diagnostics/export omit secrets and secret-shaped strings | Reuse research rules from `docs/research/2026-07-30-grok-security-acceptance.md` §3 |
| I7 | Pillow does not open Grok CLI auth files | Profile path is path-only; composition and probes never read `auth.json` |

**Wrapper type rule (implementation guidance):** secret bytes live only inside
a newtype (e.g. `SecretBytes`) that zeros on drop, never implements
`Serialize` for ordinary JSON, and is rejected by any API that would place it
in SQLite or event payloads.

### 6.2 Storage backend options

Pillow's first desktop target is **Windows**. Portability still matters for
crate boundaries.

| Backend | How it works | Pros | Cons | Portability |
| --- | --- | --- | --- | --- |
| **Windows Credential Manager (recommended primary on Windows)** | OS vault via `CredWrite`/`CredRead` (through a maintained Rust crate such as `keyring`) | User-visible OS integration; ACLs; no app-invented crypto; aligns with pillow-first "OS keychain" | Service name design needed; roaming profiles nuances; unit tests need fakes | macOS Keychain / Linux secret service via same `keyring` abstraction |
| DPAPI-encrypted blob on disk | `CryptProtectData` → file under app data | No Cred Manager dependency | App must manage files, ACLs, backup semantics; easy to accidentally log paths/contents; becomes a second durable store next to SQLite | Windows-only unless parallel implementations |
| SQLite ciphertext with app key | Encrypt secret in DB | Single file backup | Key management is the hard problem; backup *is* the secret; conflicts with AGENTS.md intent | "Portable" but weaker |
| Plaintext file / env | — | — | **Forbidden** | — |

**Recommendation:**

- **Trait:** `SecretStore { put, get, delete, exists }` in a small crate or
  module under host-side code (not in the renderer, not in `kernel-policy`).
- **Windows default:** Credential Manager via `keyring` (or equivalent),
  target name derived from opaque `secret_id` + fixed Pillow service
  namespace (e.g. service `Pillow`, user `secret:<uuid>`).
- **Tests:** in-memory fake store; never require real OS vault in unit tests.
- **Do not** implement DPAPI-file as primary; keep as last-resort research
  only if Cred Manager is blocked on a target SKU (controller decision).

SQLite holds:

```text
secret_refs
  secret_id          TEXT PK          -- UUID
  project_id         TEXT NULL        -- NULL = global/installation secret
  label              TEXT NOT NULL    -- user-visible, non-secret
  provider_kind      TEXT NOT NULL    -- e.g. 'generic_api_key', 'external_cli'
  purpose            TEXT NOT NULL    -- e.g. 'runtime_connection'
  status             TEXT NOT NULL    -- active | revoked | missing_os
  os_backend         TEXT NOT NULL    -- 'keyring' | 'none'
  content_fingerprint TEXT NULL       -- optional sha256 of secret for rotate
                                      -- collision checks ONLY; never invert
  created_at, updated_at, last_used_at TEXT
  -- NEVER: secret, token, password, ciphertext of secret
```

`content_fingerprint` is optional and controversial: it helps detect
rotate-to-same-value and corrupt OS entries without storing the secret.
**Recommendation:** store fingerprint only if computed in host memory at
enroll time and never logged; or omit in S1 to minimize secret-derived data
in SQLite (**prefer omit in S1** unless rotate UX requires it). Controller
may choose include/omit; **worker recommendation: omit in S1**.

### 6.3 Host-private use (C3 grant analogy)

C3 pattern:

```text
Host seals content + issues opaque grant
  → renderer may hold grant briefly for decide command
  → renderer never holds file content for apply
  → SQLite stores digests and decisions, not content/grant
```

S1 credential pattern:

```text
User types secret in Settings UI (transient renderer memory only)
  → single invoke: enroll_secret { label, providerKind, secret }
  → host validates, writes OS store, writes secret_refs row
  → response: { secretId, label, status }  // no secret
  → renderer drops input buffer; never re-fetches value
```

**Using** a secret later (e.g. future adapter launch):

```text
Host connection resolver
  → loads secret_id from SQLite metadata
  → SecretStore.get(secret_id) in host memory
  → passes to adapter-private launch path only
  → drops secret after spawn env construction
  → never emits secret on event bus or Tauri events
```

Renderer "uses" credentials only by selecting a **connection id** or
**secret id** (opaque). That mirrors selecting `proposalId` without supplying
file bytes.

**Transient renderer secret on enroll:** this is the one moment the secret is
in the webview. Mitigations:

- Prefer a dedicated enroll dialog that unmounts and zeroes controlled inputs
  on submit/cancel.
- Do not put the secret into React global store, URL, or DOM attributes.
- Prefer `type=password` and disable browser autofill where possible.
- No clipboard echo in success toasts.

Optional hardening (controller option): implement enroll via a **native
modal** (Rust/OS dialog) so the secret never enters the webview. **S1
recommendation:** webview enroll is acceptable for MVP if gateway and tests
are strict; native modal is a follow-up if threat model requires it.

### 6.4 Lifecycle

```text
enroll → active
active → rotate → active (new OS material; same or new secret_id — see below)
active → revoke → revoked (OS material deleted or orphaned delete)
active → delete → (row tombstone or hard delete + receipt)
revoked → delete
missing_os (OS entry gone, SQLite row remains) → re-enroll or delete
```

| Operation | secret_id policy (recommended) | OS store | SQLite | Receipt |
| --- | --- | --- | --- | --- |
| **Enroll** | New UUID | put | insert `active` | `secret_enrolled` (id, label, kind, time) |
| **Rotate** | **Keep same id** (stable references) | overwrite | update timestamps/status | `secret_rotated` |
| **Revoke** | Keep id | delete OS material | `revoked` | `secret_revoked` |
| **Delete** | Remove or tombstone | delete if present | delete or `deleted_at` | `secret_deleted` |

**Rotate keeps id (recommended):** connections point at `secret_id`; rotating
must not break foreign keys. OS backend overwrites the same target name.

**Delete receipts:** **Yes, keep a durable receipt** (append-only audit /
`secret_events` table) with secret_id, label, operation, timestamp — **never**
the secret value. Users and support need proof of deletion; C3 also keeps
rejection/expiry history rather than erasing evidence.

**Last-used timestamp:** update only from host when a launch successfully
reads the secret; never from renderer.

### 6.5 Confirmation for credential operations

| Op | Confirmation |
| --- | --- |
| Enroll | Implicit in submit; show provider kind + label; secret field required |
| Rotate | Explicit confirm; warn existing connection keeps same id |
| Revoke / Delete | Explicit confirm with typed label or one-shot **host grant** (see §7) |

### 6.6 Connection records

Separate **connection** metadata from raw secrets:

```text
runtime_connections
  connection_id     TEXT PK
  project_id        TEXT NULL REFERENCES projects(id)
  provider_kind     TEXT NOT NULL  -- 'grok_acp' | future kinds
  display_name      TEXT NOT NULL
  auth_mode         TEXT NOT NULL  -- 'external_cli' | 'secret_ref'
  secret_id         TEXT NULL REFERENCES secret_refs(secret_id)
  config_json       TEXT NOT NULL  -- non-secret adapter config only
  status            TEXT NOT NULL
  created_at, updated_at TEXT
  CHECK (
    (auth_mode = 'external_cli' AND secret_id IS NULL)
    OR (auth_mode = 'secret_ref' AND secret_id IS NOT NULL)
  )
```

**Grok default connection:** `auth_mode = external_cli`, no `secret_id`,
`config_json` holds executable/profile **paths** only (or settings keys
reference). This encodes the existing product behavior.

### 6.7 Explicit exclusion: Grok CLI auth boundary

```text
┌─────────────────────────────────────────────────────────────┐
│ Pillow                                                      │
│  - stores: executable path, profile path, adapter selection │
│  - never: opens profile\.grok\auth.json, prints tokens      │
│  - launches: Grok child with isolated HOME/USERPROFILE path │
└───────────────────────────┬─────────────────────────────────┘
                            │ spawn (path config only)
                            v
┌─────────────────────────────────────────────────────────────┐
│ Grok CLI process tree (slot / user profile)                 │
│  - owns: CLI login, auth.json, account session              │
│  - Keeper slot wrappers own multi-slot routing (read-only   │
│    from Pillow project rules; not modified by this design)  │
└─────────────────────────────────────────────────────────────┘
```

Pillow **may** show: "Grok CLI not found", "profile path missing", "adapter
probe unhealthy" using existing probe codes — **without** account labels or
auth file contents (already required by runtime composition tests).

Pillow **must not:** claim Grok slot, copy auth between slots, or treat
outer `<slot-home>\auth.json` as identity (AGENTS.md Grok CLI Slot Isolation).

## 7. Reuse of C3 Governed Primitives

C3/C4 pipeline:

```text
proposal → sealed host memory → SQLite metadata/digests →
opaque one-shot grant → human decision → host execute → durable receipt
```

| Primitive | Settings S0 | Credentials S1 | Notes |
| --- | --- | --- | --- |
| Provider-originated proposal | **No** | **No** | User-originated settings/connections UI |
| Sealed host payload | N/A (non-secret) | Secret sealed in host after enroll invoke | Renderer does not re-supply secret on later ops |
| SQLite metadata ledger | **Yes** — settings rows | **Yes** — secret_refs, events | Same DB authority |
| Opaque one-shot grant | **No** for ordinary prefs | **Yes** for revoke/delete (recommended) | Same grant digest properties: memory raw grant, SQLite stores digest only, restart invalidates outstanding grants |
| `approve_once` / `reject` vocabulary | **No** for prefs | Map to `confirm_destroy` / `cancel` in UI copy; policy crate may share grant verification helpers | Do **not** invent parallel grant crypto |
| Terminal receipt | Optional `settings_changed` | **Required** secret_* events | Append-only; no secret bytes |
| Restart invalidates pending authority | N/A for applied settings | Pending destroy grants interrupt | Same as C3 |
| Renderer cannot supply authoritative apply payload | N/A | Renderer cannot "apply secret" later; only secret_id | Same trust split |

**What not to reuse blindly:**

- Write/exec **proposal state machines** as the settings editor model — wrong
  UX and wrong actor (provider vs user).
- Conversation approval bar for settings — settings are not conversation-owned
  tool calls.
- Ten-minute proposal TTL for ordinary prefs — irrelevant.

**Shared implementation targets (recommended):**

- Extract or reuse grant mint/verify helpers (today coupled to write/exec) into
  a small host-side grant utility **or** call existing patterns without a new
  crypto design.
- Append-only event/receipt tables in SQLite with redaction discipline.
- Public error codes that never echo secrets or paths beyond already-public
  settings paths.

**Do not create a second approval framework** with different grant formats,
dual-use tokens, or renderer-trusted "admin mode."

## 8. Authority Diagram

```text
React Settings / Connections UI
  -> Tauri commands (typed DTOs; secrets only on enroll/rotate inbound)
       -> SettingsService / ConnectionService / SecretService (desktop host)
            -> storage-sqlite (settings, secret_refs, receipts; no secret bytes)
            -> SecretStore (OS keyring; secret bytes)
            -> runtime_composition / adapters (consume resolved non-secret
               config; secret bytes only inside adapter-private launch if any)
       <- DTOs without secrets
Kernel-policy
  -> unchanged C3/C4 proposal machines; optional shared grant verify for
     secret destroy confirmations
```

Desktop shell still **does not own** agent-domain logic beyond composition:
settings resolution is host infrastructure, same as `AppState` DB open.

## 9. UI / Interaction (design only)

- Settings is a **secondary surface** (Grok App baseline: infrastructure
  outside the primary conversation path).
- Conversations remain dominant; no permanent secrets dashboard.
- Connections list shows: display name, provider kind, auth mode,
  status (`connected` / `needs login` / `missing secret`), last used — never
  key material.
- Enroll uses a modal; success toast names the label only.
- Advanced "Open Grok profile folder" may reveal a path the user already set;
  never auto-open auth files.

## 10. Testing Requirements (when implementation is authorized)

S0:

1. Migration applies cleanly on empty and on post-C4 schemas.
2. Precedence: default < global < project < env allow-list.
3. Invalid adapter enum fails closed (no Fake).
4. Restart restores settings.
5. Project delete policy for project-scoped settings (cascade or block —
   **recommend cascade delete project settings with project**).
6. No column/API accepts secret-shaped values into settings.

S1:

1. Enroll → OS exists → SQLite ref → list DTO has no secret.
2. Canary secret never appears in SQLite file bytes, logs, or invoke responses.
3. Rotate keeps secret_id; old material unusable.
4. Revoke/delete removes OS material; receipt remains.
5. Missing OS entry surfaces `missing_os` without panic/leaks.
6. Grok `external_cli` connection works with zero secret_refs.
7. Renderer gateway strips forbidden fields (mirror write grant tests).
8. Process.run still cannot receive secret env from provider/renderer.

## 11. Implementation Phasing (not authorized by this doc alone)

| Rung | Deliverable | Exit |
| --- | --- | --- |
| **S0** | SQLite settings + service + UI + composition read path | External Grok users configure paths without env; tests green |
| **S1** | SecretStore + secret_refs + enroll UI + receipts + redaction tests | Can hold one API-key class secret without leakage; Grok remains external_cli |
| **S2+ (separate designs)** | Wire secrets into new adapters; optional native enroll dialog; export/import | Per-adapter designs |

## 12. Out of Scope Checklist

- Product source, tests, scripts, config changes (this task is design-only).
- Git operations, builds, desktop E2E, commits.
- Reading or printing any auth/token file.
- `.superpowers/` mutation.
- Obsidian, company OS, MCP network tools, auto-approve.
- Taking ownership of Grok CLI or Keeper slot credentials.
- Injecting secrets into C4 process environment.
- Cloud account system for Pillow itself.

## 13. Open Questions for Controller

| ID | Question | Options | **Recommendation** |
| --- | --- | --- | --- |
| Q1 | Ship S0 before S1 as separate implementation plans? | (a) Yes, two plans (b) One plan two milestones | **(a)** — unblocks composition cleanup without keyring work |
| Q2 | Allow `fake` adapter in production settings UI? | (a) Hidden behind env-only (b) Advanced settings with warning (c) Dev builds only | **(c)** or **(a)** — never a casual UI default |
| Q3 | Project-scoped secrets vs global-only in S1? | (a) Global only (b) Global + project (c) Project only | **(b)** — project isolation matches product model; default UX can still create global |
| Q4 | Secret destroy confirmation: full C3 grant vs simple modal confirm? | (a) Modal only (b) Host one-shot grant like C3 (c) Typed label + modal | **(c)** for S1 speed; upgrade to **(b)** if multi-window race becomes real |
| Q5 | Store secret content fingerprint in SQLite? | (a) Omit (b) Store sha256 | **(a)** omit in S1 |
| Q6 | Enroll surface | (a) Webview password field (b) Native dialog | **(a)** for S1; schedule **(b)** if security review demands |
| Q7 | Should S0 immediately remove env authority or keep env override forever? | (a) Env override retained (b) Env ignored when DB set (c) Env only in test builds | **(a)** retain allow-listed overrides |
| Q8 | Next free migration number / table names | — | Controller assigns against live tree at implement time (`0009+`) |
| Q9 | Is interim external delivery allowed on S0 + Grok CLI auth only? | (a) Yes with explicit claim (b) No, wait for S1 | **(a)** if marketing claim stays Grok-CLI-auth-only |

## 14. Acceptance Criteria for This Design

Controller may accept this design when:

1. Judgments §4.1–§4.4 are explicitly accepted or amended in writing.
2. Security invariants I1–I7 are accepted.
3. C3 reuse stance (§7) is accepted — no second approval framework.
4. Grok CLI non-takeover (§6.7) is accepted.
5. S0/S1 scope and exclusions match product order (Stage 1 only).
6. Open questions Q1–Q9 are decided or deferred with owners.

Implementation requires a separate plan and authorization. This document alone
does **not** authorize product code changes.

## 15. References

- `AGENTS.md` — SQLite authority; approval/audit/secret below UI; no committed credentials; Grok slot isolation
- `docs/superpowers/specs/2026-07-29-pillow-first-design.md` — OS keychain + opaque IDs; Stage order
- `docs/superpowers/specs/2026-08-01-c3-governed-write-design.md` — proposal/grant/receipt; host-private content
- `docs/superpowers/specs/2026-08-02-c4-governed-process-git-design.md` — env denylist; no credentials in C4.0
- `docs/research/2026-07-30-grok-security-acceptance.md` — zones Z0–Z5; where secrets may live
- `docs/migration/legacy-capability-inventory.md` — port secret separation via OS keychain
- `apps/desktop/src-tauri/src/runtime_composition.rs` — current env-only runtime selection
- `crates/storage-sqlite/` — migration and repository patterns
- `crates/kernel-policy/` — write/exec approval state machines (reuse patterns, not settings UX)

---

**Signal: SETTINGS-DESIGN-COMPLETE**
