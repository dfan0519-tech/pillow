# C4 Git Governance Research

Date: 2026-08-02
Researcher: grok2 (slot `grok2`)
Workspace: `<workspace>\pillow`
Baseline HEAD inspected: `7b7f70f` (accepted C1–C3 dirty working tree; staging empty)
Scope: project-scoped Git capability ladder design evidence only — **no product implementation**

Keeper Bridge: session boot for `project=pillow`, task `C4 Git governance research`, `agentName=grok2`, `agentRole=controller` (PowerShell `Invoke-RestMethod`; curl alias rejected).

---

## 1. Executive recommendation

**Ship C4 Git as a two-rung ladder under the same authority model as C2 resources and C3 write proposals.**

| Rung | Surface | Authority |
| --- | --- | --- |
| **C4-G0** (first ship) | Repository discovery, status, bounded file diff, branch/HEAD identity, worktree *read* identity, explicit refresh | Project-scoped **read-only** host commands; no approval; no mutation |
| **C4-G1** (second ship, separate design gate) | Path-bounded stage, unstage, commit, safe branch switch, single-file restore | C3-style **proposal → approve_once → CAS apply → receipt**; one active Git mutation per conversation/run |

**Do not ship** in C4 (any rung): `reset --hard`, `clean`, force checkout, rebase, merge, cherry-pick, stash drop of others’ work, push/publish/fetch with network side effects, remote rewrite, submodule update --init as a side effect of status, worktree add/remove/gc, or arbitrary Git argv / shell passthrough.

**Implementation approach for Windows C4-G0/G1:** prefer a **supervised Git CLI executor with fixed argv templates** (same pattern as pinned Grok App), not direct `git2`/`gix` write paths, until a later performance or offline-git design reopens native libraries. See §8.

**Provenance stance:** Grok App and zbbody remain **conceptual** references. No application source copy. Any later adaptation must record commit, path, license, and notice in `THIRD_PARTY_NOTICES.md`.

---

## 2. Evidence map

### 2.1 Accepted Pillow C2 resource boundary

| Claim | Local evidence |
| --- | --- |
| Conversation stays visual center; resources are collapsible contextual pane | `docs/verification/2026-08-01-wave-c2-checkpoint.md`; `docs/roadmap/2026-07-31-stage1-overall-status.md` C2 table |
| Project-scoped list/read/search/recents/preview; no native roots in renderer | `apps/desktop/src/lib/resourceGateway.ts` (portable paths only); `crates/kernel-resource` |
| Portable path grammar + reparse/symlink rejection | `crates/kernel-resource/src/lib.rs` `validate_portable_path` / `resolve_target` |
| Durable recents are portable paths keyed by `project_id` | `crates/storage-sqlite/migrations/0004_resource_recents.sql` |
| C2 excludes terminal and Git | C2 checkpoint “Next Stage”; stage1 status “terminal and Git stay deferred” |

### 2.2 Accepted Pillow C3 write/diff/approval boundary

| Claim | Local evidence |
| --- | --- |
| One bounded UTF-8 create/modify; host diff; approve_once; base-hash CAS; atomic apply; SQLite metadata-only | `docs/superpowers/specs/2026-08-01-c3-governed-write-design.md` (accepted) |
| Provider proposes; host injects identity; renderer never supplies content/roots/digests to apply | Spec §4 authority diagram; `writeApprovalGateway.ts` |
| Denied write components include `.git` | Spec §8.1; `crates/tool-executor/src/write.rs` `DENIED_COMPONENTS` |
| Explicit C3 exclusions: terminal, Git, credentials, multi-file, binary, rename/delete | Spec §3.2; `docs/verification/2026-08-02-wave-c3-checkpoint.md` “Still excluded” |
| Lifecycle + grant digest anti-replay + restart → interrupted | Spec §5–§9; migrations `0005_write_proposals.sql` |
| Diff caps: 262,144 bytes / 4,000 lines; content cap 262,144 UTF-8 bytes | Spec §3.1 / §8.2 |

C4 Git **must not** widen C3 write authority. Git mutations are a **separate proposal family**, not a loophole around `fs.write@1`.

### 2.3 Pinned Grok App Git flows

| Item | Value |
| --- | --- |
| Local clone | `<workspace>\VaultSystem\references\grok-app` |
| Pinned commit (verified) | `cbbbdf17bb8db1588b6707f3f42f6b00397e27f9` |
| License | MIT (`docs/references/upstream-baseline.md`) |

Host commands (`src-tauri/src/commands.rs`, registered in `lib.rs`):

| Command | Behavior (conceptual) |
| --- | --- |
| `git_status` | Soft-fail; `git -C` + `status --porcelain=v1 -z --untracked-files=normal`; branch via `rev-parse --abbrev-ref HEAD`; cap 2000 entries; absolute path joined for UI |
| `git_file_diff` | `diff --no-color --no-ext-diff HEAD -- <rel>`; untracked via `--no-index`; truncate ~400k chars |
| `git_show_file` | HEAD blob content for “before” side of local diffs |
| `git_worktrees_list` / `add` / `remove` / `gc` | Worktree porcelain + mutating worktree ops |

Frontend pure helpers:

- `src/lib/workspaceGit.ts` — porcelain kind classification (`modified|added|deleted|untracked|renamed|copied|typechange|conflict|ignored|unknown`); safe-discard candidate (unstaged tracked only)
- `src/lib/gitWorktree.ts` — worktree porcelain parse; prune argv builder (path sanitization: no leading `-`, limited charset)
- `src/lib/api.ts` — renderer invoke wrappers; **projectPath is a machine path string**

UI model: Session Changes (tool-derived) + Workspace Changes (`git_status`); ResourceViewer diff cascade: payload before/after → `git_file_diff` → HEAD+after (C3 research already documented this cascade).

**Adopt as patterns:** soft-fail availability; porcelain `-z` parsing; kind taxonomy; fixed argv; entry caps; worktree identity *read*.

**Reject as Pillow boundaries:** renderer-held absolute project paths; unbounded worktree add/remove/gc as product defaults; stderr snippets that may leak host paths to UI; ungoverned mutations (Grok App status/diff are largely read-oriented; mutating worktree GC is too broad for C4).

### 2.4 zbbody license and Git-adjacent surface

| Item | Value |
| --- | --- |
| Local path | `<workspace>\zbbody` |
| Inspected commit | `00c293414333b6fcc4e4598389fa3952c8d0b09b` |
| LICENSE | Apache License 2.0 (file header + `package.json` `"license": "Apache-2.0"`) |
| Readme provenance | Continues AionUi under Apache-2.0 |

Git-adjacent concepts (not Pillow authority):

- `packages/desktop/src/common/types/platform/fileSnapshot.ts` — `staged` / `unstaged` `FileChangeInfo`; `mode: 'git-repo' | 'snapshot'`; branch nullable
- `ipcBridge.ts` `fileSnapshot` — HTTP backend routes: `stage`, `stageAll`, `unstage`, `unstageAll`, `discard`, `reset`, `getBranches`
- Workspace string is the identity key for snapshot APIs

**Adopt (concept only):** staged vs unstaged list split; open-file vs open-diff dual affordance; progressive disclosure of diffs.

**Reject:** workspace-string as durable project authority; stage-all / unstage-all / discard / reset without C3-grade proposal+CAS; Electron HTTP/WebSocket renderer discovery; YOLO discard.

**Direct reuse recommendation:** **none**. Conceptual interaction only. Apache-2.0 would permit adapted implementation *if* a later brief names exact paths, commit, and attribution — still not recommended for security-critical Git mutation code.

### 2.5 Pillow seam map (canonicalization → pane)

```text
path_scope (picker, disk-only Windows, reparse reject, TTL grants)
  -> open_or_register_project
  -> SQLite projects(id, root_path, canonical_root_path UNIQUE, trust_state)
  -> ProjectCapability / ProjectResourceCapability (canonical root)
       |
       +-- resource_commands / ResourceGateway  [C2 read-only UI]
       +-- tool-executor governed read/write   [B3/C3 run path]
       +-- conversation_service project_cwd    [run binding]
       |
runtime-host + kernel-policy + write_proposals SQLite
  -> writeApprovalGateway + Workbench approval bar + contextual diff pane
```

| Seam | C4 Git attachment |
| --- | --- |
| **Project id + canonical root** | All Git discovery scoped to `projects.canonical_root_path`; never accept raw path from renderer |
| **Portable relative paths** | Status/diff paths are project-relative portable grammar (same as resource/write); repo-relative paths mapped and re-validated under project root |
| **Approvals** | New `git_*` proposal types parallel `write_proposals` — not reuse of `fs.write` rows |
| **Receipts** | Metadata + op digests + terminal result_json in SQLite; no commit message body secrets beyond bounded stored message digest + optional redacted preview policy |
| **SQLite** | Domain authority remains SQLite; Git object database is external truth for tree content, not Pillow domain store |
| **Contextual pane** | Status list + file diff open through existing collapsible resource/contextual pane; no permanent Git dashboard |
| **resourceGateway** | Stays read-only; Git mutations never go through resource commands |

Windows path policy already locked (`path_scope.rs`): absolute disk prefix only; no UNC/parent; reparse reject on picker; canonicalize + VerbatimDisk normalize.

---

## 3. Capability ladder (product order)

### 3.1 C4-G0 — Read-only Git observation

**Goal:** Answer “what is dirty under this project?” without mutation.

| Capability | Host API (conceptual) | Provider tool? |
| --- | --- | --- |
| Discover repositories | `git.discover@1` | Optional later; UI-first is enough |
| Status | `git.status@1` | Optional `git.status` read tool |
| File diff | `git.diff@1` | Optional |
| Branch / HEAD / detached | included in status/identity DTO | — |
| Worktree list (read) | `git.worktrees@1` | — |
| Refresh | client-triggered + invalidation events | — |

No approval. Rate-limit and size-bound only.

### 3.2 C4-G1 — Narrowly governed mutations

| Operation | Semantic intent | Git CLI template (illustrative; fixed host only) |
| --- | --- | --- |
| Stage paths | Add exact paths to index | `git -C <root> add -- -- <paths…>` |
| Unstage paths | Remove paths from index only | `git -C <root> restore --staged -- -- <paths…>` (or `reset -q HEAD -- <paths>` if restore unavailable; host picks one strategy and tests it) |
| Commit | Create commit from current index | `git -C <root> commit -F <host-temp-msgfile>` with no `-a`, no `--amend`, no `--no-verify` by default policy |
| Switch branch | Move HEAD to existing local branch when safe | `git -C <root> switch -- <branch>` (no `--force`, no pathspecs) |
| Restore single file | Discard *worktree* changes for one tracked path to HEAD | `git -C <root> restore --source=HEAD --worktree -- -- <path>` |

Each mutation is a **proposal** with fingerprint, grant, expiry, CAS preconditions (§6).

### 3.3 Explicit exclusions (hard)

| Family | Examples | Why excluded |
| --- | --- | --- |
| Destructive mass reset | `reset --hard`, `clean -fdx` | Irreversible data loss; no CAS identity for “everything” |
| Force checkout | `checkout -f`, `switch -f` | Overwrites dirty trees |
| History rewrite / integration | rebase, merge, cherry-pick, revert multi-commit | Conflict graphs + multi-file identity exceed C4 |
| Network publish | push, pull, fetch, clone remote | Credentials + network boundary (later stage) |
| Worktree mutation | add/remove/move/gc/prune | Cross-root side effects outside project model |
| Submodule mutation | update --init, deinit, absorbgitdirs | Nested trust + network |
| Credential / identity forge | embedding tokens; agent-chosen `user.email` without policy | Secret + attribution boundary |
| Argv passthrough | `git <user string>` / `Bash(git *)` YOLO | Breaks allowlist audit |

---

## 4. Required decisions

### 4.1 Repository discovery below a project root (including nested)

**Decision:**

1. **Primary discovery root** is the project’s `canonical_root_path` only.
2. A path is a Git work tree if `git -C <dir> rev-parse --is-inside-work-tree` is `true` **or** a bounded native probe finds a `.git` file/directory after reparse-safe walk (CLI preferred for consistency).
3. **Nested repositories:** bounded depth-first or BFS walk under the project root:
   - max depth (recommend **8**);
   - max repos returned (recommend **32**);
   - skip ignored junk only after safe name filters (always skip symlink/reparse components entirely, matching resource policy);
   - never follow junction/symlink out of canonical root;
   - treat `.git` **file** (worktree/gitdir pointer) and `.git` **directory** as repo markers; do not open nested `.git/objects` as project resources.
4. **Active repository** is user- or host-selected among discovered repos; default:
   - if project root itself is inside a work tree → that work tree’s top-level (`rev-parse --show-toplevel`) if toplevel is still under project root;
   - else first nested repo by portable path sort order;
   - if toplevel escapes project root (repo root above project) → **available=false** with public code `git_root_outside_project` (no path in message).
5. **Submodules:** appear as nested repos if checked out; status of superproject shows `kind=submodule` entries without auto-recursing into submodule mutation.
6. **Discovery is not durable domain state.** Optional cache: `(project_id, discovery_generation, repos_json digest)` in memory; invalidate on project switch, root revalidate failure, or explicit refresh. Do not store machine paths in SQLite beyond existing project roots.

**Nested repo path identity:** each repo DTO carries:

- `repoId`: stable hash of `project_id + portable_repo_root_rel` (or `.` for root);
- `rootRelativePath`: portable path from project root to work tree root (`"."` if coincident);
- never export absolute paths to the renderer.

### 4.2 Stable status/diff DTOs and binary/large-file bounds

**Status DTO (host → renderer/gateway):**

```ts
type GitRepoIdentity = {
  projectId: string;
  repoId: string;
  rootRelativePath: string; // portable; "." allowed
  available: boolean;
  reasonCode?: GitPublicReasonCode; // opaque, path-free
  branch: string | null;            // null if detached or unavailable
  headShaShort: string | null;      // 7–12 hex; never full object dump
  detached: boolean;
  upstream: string | null;          // optional G0.1; null if none
  ahead: number | null;
  behind: number | null;
  conflicted: boolean;
  discoveryGeneration: number;
};

type GitStatusEntry = {
  path: string;              // project-portable path
  repoRelativePath: string;  // within active repo
  status: string;            // two-char porcelain XY
  indexStatus: string;       // one char
  worktreeStatus: string;    // one char
  kind: GitStatusKind;
  originalPath?: string;     // rename/copy source, portable
  isBinary?: boolean;        // best-effort
  sizeBytes?: number;        // worktree size when cheap
};

type GitStatusKind =
  | "modified" | "added" | "deleted" | "untracked"
  | "renamed" | "copied" | "typechange" | "conflict"
  | "submodule" | "ignored" | "unknown";

type GitStatusView = {
  identity: GitRepoIdentity;
  files: GitStatusEntry[];
  truncated: boolean;
  entryCount: number;
  capturedAt: string; // ISO
};
```

**Bounds (G0 defaults):**

| Resource | Cap | Behavior when exceeded |
| --- | --- | --- |
| Status entries | **2000** (align Grok App) | `truncated: true`; stable sort by path; drop tail |
| Single diff payload | **262,144 bytes** and **4,000 lines** (align C3) | `truncated: true` + marker; still return metadata |
| Diff request path count | **1 path per call** in G0 | Multi-path deferred |
| `git show` / before content | **262,144 bytes** | truncate or mark binary |
| Status wall time | **5s** default | soft-fail `git_timeout` |
| Concurrent status per project | **1** in-flight | coalesce callers |

**Binary / large-file:**

- Detect binary via Git’s textconv-free heuristics (`git diff --numstat` / `--raw` or NUL scan of first 8 KiB).
- Binary status rows stay listable; `git.diff` returns `kind: "binary"` with **no** unified text (or only `Binary files differ` line).
- Files larger than content cap: status includes `sizeBytes`; diff not materialised.

**Public reason codes (path-free):**
`git_unavailable`, `not_a_repository`, `git_root_outside_project`, `git_timeout`, `git_limit_exceeded`, `path_rejected`, `repo_not_found`, `unsupported`.

### 4.3 Rename, submodule, symlink, ignored, untracked semantics

| Case | Status semantics | Diff | Mutation (G1) |
| --- | --- | --- | --- |
| **Rename/copy** | `kind=renamed|copied`; `originalPath` + new `path`; porcelain `-z` two-record parse (Grok App pattern) | Diff against rename detection as Git reports; if truncated, still show both paths | Stage/unstage must include **both** sides when Git requires; proposal lists explicit path set host expands from selection |
| **Submodule** | Superproject entry `kind=submodule`; do not flatten submodule dirty files into parent without separate `repoId` | No recursive submodule diff in G0 | Stage submodule SHA only as Git stages; **no** `submodule update` |
| **Symlink / reparse** | If Git tracks symlink: show as typechange/modified per porcelain; **Pillow path resolve still rejects reparse for resource open** | Diff may show mode change; content open via resource gateway may fail closed | Restore/stage of symlink paths allowed only if portable path resolves without escaping root; reparse-as-directory escape → `path_rejected` |
| **Ignored** | Default status **excludes** ignored (`status` without `-u` all + without ignored). Optional G0 flag `includeIgnored=false` default | N/A | Never stage ignored via silent path expansion; explicit path stage uses `git add` rules (may force-add only if future policy adds `force` — **default off**) |
| **Untracked** | `??` / `kind=untracked` | Diff as full addition via `--no-index` or host synthetic add-diff with same caps | Stage = `git add` path; restore **does not delete** untracked (restore applies to tracked only); deleting untracked is **out of scope** (would be clean) |

### 4.4 Branch / worktree identity and refresh / invalidation

**Identity fields:** see `GitRepoIdentity` above. Detached HEAD ⇒ `detached=true`, `branch=null`, short SHA shown.

**Worktrees (read-only G0):**

- List via `git worktree list --porcelain` (parse as Grok App `gitWorktree.ts`).
- Expose sibling worktrees **only when path stays under the same project root**; external worktrees → omit or mark `outside_project` without path leak.
- **No** worktree add/remove/gc in C4.

**Refresh / invalidation rules:**

| Event | Action |
| --- | --- |
| Project switch | Drop all in-memory Git views; cancel in-flight discovery/status (generation counter) |
| Conversation switch | Do not leak foreign conversation Git proposal bars; status cache may be project-scoped shared read |
| Successful G1 mutation receipt | Bump `discoveryGeneration`; force status refresh |
| Successful C3 write apply under repo | Bump generation (worktree may have changed) |
| Resource root revalidation failure | Git available=false |
| App restart | Memory grants/proposals interrupted (same as C3); status rediscovered |
| External FS change | No mandatory file watcher in G0; UI pull-to-refresh / focus refresh only (watcher = later optional) |

Renderer holds **no** absolute paths; refresh tokens are opaque integers.

### 4.5 Exact approval payloads (G1)

Mirror C3: host seals proposal; renderer decides with `proposalId + approvalGrant + decision` only.

```ts
type GitMutationKind =
  | "stage"
  | "unstage"
  | "commit"
  | "switch_branch"
  | "restore_file";

type GitMutationProposalView = {
  proposalId: string;
  conversationId: string;
  projectId: string;
  repoId: string;
  kind: GitMutationKind;
  state: WriteProposalState; // reuse lifecycle enum
  summary: string;           // path-free or portable-path summary only
  expiresAt: string;
  approvalGrant?: string;    // raw grant in memory only
  // kind-specific review fields (no roots, no credentials):
  paths?: string[];          // portable, max N
  branch?: string;           // switch target (validated name)
  messagePreview?: string;   // commit: bounded preview, not full secret dump policy
  baseFingerprint: string;   // sha256:… of sealed preconditions
};

// Decide command — same shape as write:
// { conversationId, proposalId, approvalGrant, decision: "approve_once" | "reject" }
```

**Fingerprint inputs (host-canonical, deny unknown fields):**

| Kind | Fingerprint covers |
| --- | --- |
| `stage` / `unstage` | `projectId, repoId, kind, sorted portable paths, per-path content digest (worktree) + index blob id if present, HEAD sha, index tree sha` |
| `commit` | `projectId, repoId, kind, HEAD sha, index tree sha, message digest, author-mode policy id, path list optional empty` |
| `switch_branch` | `projectId, repoId, kind, fromHead, toBranch, toHead, dirtyTreeDigest (or clean flag)` |
| `restore_file` | `projectId, repoId, kind, path, worktree digest, HEAD blob id, index state` |

**SQLite (conceptual migration `0007_git_mutations.sql`):** metadata/digests/state/decisions/receipts only — **not** full file contents; commit message stored as digest + optional bounded preview column (≤ 2 KiB) if product needs timeline text; raw grant never stored.

**Cardinality:** one active non-terminal Git mutation proposal per run (and recommend per conversation for UI clarity), same as write proposals.

**TTL:** 10 minutes (reuse `WRITE_PROPOSAL_TTL_SECS` policy constant or shared `PROPOSAL_TTL_SECS`).

### 4.6 Dirty-tree, conflict, detached-HEAD, base-drift

| Condition | G0 | G1 stage/unstage | G1 commit | G1 switch | G1 restore |
| --- | --- | --- | --- | --- | --- |
| Clean tree | OK | OK (no-op may succeed with receipt) | Fail `nothing_to_commit` if index empty of changes | OK | Fail if file already matches HEAD |
| Dirty tree | Listed | OK for selected paths | OK if index has staged changes | **Fail closed** unless policy `switch_allow_if_unrelated` (default **deny**) | OK for selected path if preconditions match |
| Conflicted index | `conflicted=true`; entries `kind=conflict` | Allow only explicit path ops that Git accepts; prefer fail with `conflict_present` for commit/switch | **Deny commit** | **Deny switch** | Restore conflicted file only as explicit single-path restore proposal (still no `merge --abort`) |
| Detached HEAD | Identity shows detached | Stage/unstage OK | Commit OK (creates detached commit) — UI must label risk; optional policy deny | Switch to branch OK if clean | OK |
| Base drift (CAS fail) | N/A | Fail `base_changed`; no partial apply | Fail if HEAD or index tree ≠ sealed | Fail if HEAD ≠ sealed | Fail if worktree digest ≠ sealed |
| Concurrent second mutation | N/A | Conflict reservation | same | same | same |

**Partial failure:** stage multi-path is either transactional best-effort documented as **all-or-nothing host attempt** (run one `git add -- paths` and re-status; if incomplete, `failed` receipt with opaque code — no silent half-success UI). Prefer **max 64 paths per proposal**.

### 4.7 Commit-message and author / credential boundaries

| Concern | Decision |
| --- | --- |
| Message source | Provider or human may propose text; host seals bytes ≤ **8 KiB**, no NUL, Unicode scalar check; strip trailing whitespace only as Git does via file |
| Message in approval UI | Show full message up to cap in contextual pane; timeline summary first line ≤ 72 chars |
| `--trailer` / signoff | Out of scope unless explicit later policy |
| Author / committer | **Use repository/Git config identity only** (`user.name` / `user.email` already configured). Do **not** accept provider-supplied author. If identity missing → fail `git_identity_missing` with fixed help text (no config path leak) |
| `GIT_AUTHOR_*` env | Host executor sets **clean env** blocklist: strip `GIT_TRACE*`, credential helpers injection from provider; do not pass renderer env |
| Signing (`-S`) | Default off; GPG/ssh signing out of C4 |
| Hooks | Default **run hooks** (`commit` without `--no-verify`). If hook fails → `failed` receipt, no retry amplification. Future policy may add `skip_hooks` only with separate approval kind |
| Credentials | **Never** prompt, store, or forward credentials. No `credential.helper` override from Pillow. Push not in scope |
| Commit amend | **Excluded** (`--amend` can rewrite published history) |

### 4.8 Exclusions (restate as testable deny list)

Host argv allowlist must **hard-deny** (unit tests with fixture argv builder):

- `reset` with `--hard` / `--merge` / `--keep` destructive modes; prefer no `reset` verb at all if `restore` covers unstage
- `clean`
- `checkout` / `switch` with `-f` / `--force` / pathspecs that overwrite
- `rebase`, `merge`, `cherry-pick`, `revert` (multi)
- `push`, `pull`, `fetch`, `clone`, `ls-remote`
- `worktree add|remove|move|prune`
- `submodule update|deinit|add`
- `config` writes, `credential`, `send-email`
- Any bare `--` missing path guard that allows option injection (`--upload-pack`, `-c`, `-c core.sshCommand=…`)
- Provider/user free-form argument arrays

**Option injection defense:** all user-controlled strings after `--`; branch names validated against `^[A-Za-z0-9._/+\-]+$` with length cap 255 and no leading `-`; paths validated by portable grammar + resolve under repo root.

### 4.9 Test fixtures and adversarial acceptance scenarios

**Fixtures (temp dirs under test harness, not user vaults):**

1. Clean repo with one commit
2. Dirty modified tracked file
3. Staged + unstaged same file (`MM`)
4. Untracked file
5. Rename (`git mv`)
6. Conflict (merge prepared, leave conflicted)
7. Detached HEAD
8. Nested second repo under `vendor/nested/.git`
9. Repo root *above* project folder (project is subfolder of repo)
10. Symlink/reparse attempt to escape (Windows junction)
11. Binary file + large text > 256 KiB
12. Missing `git` on PATH
13. Empty commit message / huge message
14. Path with spaces and Unicode NFC/NFD names

**Adversarial acceptance (desktop + unit):**

| ID | Scenario | Expected |
| --- | --- | --- |
| G-A1 | Renderer invents absolute path in status request | Reject / ignore; only `projectId`+relative |
| G-A2 | Stage path outside repo via `../` | `path_rejected` |
| G-A3 | Approve grant replay | Conflict; no second mutation |
| G-A4 | Drift worktree after approve before apply | `base_changed` |
| G-A5 | Cross-conversation decide | ownership mismatch |
| G-A6 | Commit while conflicted | denied |
| G-A7 | Switch while dirty | denied (default) |
| G-A8 | Restore untracked as delete | denied (not restore) |
| G-A9 | Proposal for `reset --hard` kind | schema reject |
| G-A10 | Nested repo mutation without selecting its `repoId` | fail closed |
| G-A11 | Project A status while Project B selected | no leakage |
| G-A12 | Restart mid-approved commit | interrupted; no auto-commit |
| G-A13 | Diff binary does not dump base64 megabytes | binary marker only |
| G-A14 | stderr from git not shown raw in UI | opaque codes |
| G-A15 | Concurrent stage + C3 write on same path | both CAS independently; at least one fails closed on conflict |

**Protocol tests:** fixed manifests; deny_unknown_fields; fingerprint conflict vs replay (reuse tool reservation lessons from C3 research).

---

## 5. Authority model (aligned to C2/C3)

```text
Provider (optional later)
  -> may propose git.stage|unstage|commit|switch|restore arguments only
Adapter
  -> schema validate; no roots; no argv arrays
Runtime host
  -> inject project/conversation/run; seal preconditions; create grant
SQLite
  -> proposal metadata, digests, transitions, decision grant digest, receipts
Conversation UI
  -> list/decide/open review DTO; contextual status/diff pane
Git executor (host)
  -> fixed argv templates; clean env; timeout; job-object kill (Windows)
  -> re-check CAS; apply; re-status; persist receipt
```

**UI disclosure (conversation-centred):**

```text
timeline summary → single approval bar (G1) → contextual Git status/diff pane → terminal receipt row
```

No permanent multi-card Git dashboard. Resource pane remains collapsible (C2-C1 lesson).

**Provider tools:** G0 may ship **UI-only** without provider tools. If provider read tools are added, they are governed-read style (reservation + caps), not shell.

---

## 6. Implementation approach comparison (`git2` / `gix` / CLI)

Pillow currently has **no** `git2`/`gix` dependency (workspace `Cargo.toml` search empty).

| Dimension | Supervised Git CLI | `git2` (libgit2) | `gix` (gitoxide) |
| --- | --- | --- | --- |
| Windows status/diff parity | Strong — same porcelain users already trust; Grok App proof at pinned commit | Strong for status/diff APIs | Good and improving; not full feature mirror of CLI porcelain edge cases |
| Mutation safety | Excellent with argv allowlist + no shell | Full API surface easy to misuse (checkout force, etc.) | Same risk if wide API exposed |
| Process isolation | Fits C4 terminal process model (timeout, job object) | In-process; bugs = host crash | In-process |
| Credential accidents | Avoid by never calling network verbs | Credential callbacks can surprise | Similar |
| Deploy cost | Requires `git` on PATH (soft-fail if missing) | Links native lib; MSVC/build complexity | Pure Rust; larger compile graph |
| Offline / no Git install | Fail soft | Works if linked | Works if linked |
| Rename `-z` / submodule quirks | Match user Git version | libgit2 version skew vs system Git | Version skew vs system Git |
| C4-G0 fitness | **Best default** | Good alternative for read if Git missing | Viable read alternative later |
| C4-G1 fitness | **Best default** (templates + CAS outside Git) | Possible but higher misuse surface | Incomplete write ergonomics historically |

**Recommendation:**

1. **C4-G0/G1:** supervised CLI executor module `git-executor` (name TBD) with:
   - `GitArgvTemplate` enum (closed);
   - `git` binary discovery once per session;
   - timeout + stdout/stderr byte caps;
   - Windows Job Object kill on cancel (coordinate with grok1 terminal research);
   - never `cmd.exe /c`.
2. **Do not** add `git2`/`gix` in the first implementation PR unless CLI is proven insufficient for discovery performance.
3. Optional later **hybrid:** `gix` for pure status parsing if dependency review passes — still no free-form mutations.

---

## 7. Suggested staged delivery (research only; not authorized)

| Stage | Deliverable | Gate |
| --- | --- | --- |
| C4-G0a | Discover + identity + status DTO + Tauri/TS gateway + pane list | Unit porcelain fixtures; two-project isolation; no mutation symbols in binary allowlist tests |
| C4-G0b | Bounded file diff + binary behavior + refresh generations | Diff caps; ResourceViewer-style open from status |
| C4-G1a | Stage/unstage proposals + approval UI reuse | Approval-path tests mandatory (AGENTS rule) |
| C4-G1b | Commit + identity missing handling | Message/author policy tests |
| C4-G1c | Switch (clean only) + single-file restore | Dirty/conflict deny matrix |
| C4-Gx | Explicit non-goals regression pack | Adversarial list G-A1–G-A15 |

Terminal (grok1) and interaction (grok3) and security (grok4) reports must be integrated by the controller before any implementation brief.

---

## 8. Interaction with C3 writes

- C3 continues to **deny writes under `.git`**.
- Git commit does **not** replace governed file write: agent still needs C3 to edit files; G1 only records already-present worktree/index state.
- Ordering: typical flow = C3 apply(s) → G0 status → G1 stage → G1 commit, each with own approvals.
- Same path concurrent C3 apply + G1 restore: both re-read digests; one wins; the other `base_changed` / `failed` — fail closed.

---

## 9. Provenance and reuse matrix

| Source | Commit | License | Use in C4 Git |
| --- | --- | --- | --- |
| Grok App | `cbbbdf17bb8db1588b6707f3f42f6b00397e27f9` | MIT | Conceptual: porcelain status/diff/worktree read, caps, soft-fail |
| zbbody | `00c293414333b6fcc4e4598389fa3952c8d0b09b` | Apache-2.0 | Conceptual: staged/unstaged split, discard caution; **no source copy** |
| Pillow C2/C3 | dirty tree @ HEAD `7b7f70f` | project | Binding authority, path grammar, approval ledger patterns |
| Legacy Keeper approval | frozen reference | — | Invariants already ported into C3; reuse patterns not JS |

---

## 10. Residual risks

1. **System Git version skew** on Windows (restore vs reset -q; switch vs checkout) — mitigate with capability probe at executor init and fixture matrix on min Git version (document floor, e.g. Git 2.36+).
2. **Nested repo / monorepo UX** complexity — default selection rules may surprise; keep explicit repo picker in pane.
3. **No FS watcher** in G0 → stale status after external edits — acceptable if refresh is obvious.
4. **Hooks on commit** may hang or call network — hard timeout + kill required (pair with process research).
5. **Line-ending / smudge filters** can make CAS digests disagree with naive file reads — CAS must use same read path as Git (hash-object or re-status) not only Pillow UTF-8 write digest.
6. **Parallel C4 terminal shell** could run forbidden git if shell ships open — Git allowlist does not protect a general shell; controller must keep shell command allowlists independent (grok1/grok4).
7. **Research-only:** this document does not authorize implementation, dependency adds, staging, or commit.

---

## 11. Controller integration notes

Reject any implementation brief that:

- exposes absolute paths to the renderer;
- adds free-form `git` argv;
- reuses `write_proposals` rows for Git without a distinct kind/schema;
- enables push/force/clean/hard-reset;
- skips approval-path tests for G1;
- treats zbbody/Grok App as copy-paste sources without provenance.

---

## Finish block

**Status: DONE**

**Changed files (truth):**

- Created `<workspace>\pillow\docs\research\2026-08-02-c4-git-governance-research.md` only.

**Residual risks:** see §10 (Git version skew, nested-repo UX, hook hangs, smudge/CAS, general shell bypass). No product code or reference repos modified.

**Keeper signal:**
`C4-Grok2-GitGovernance-DONE — ladder G0 read discovery/status/diff/branch + G1 governed stage/unstage/commit/switch/restore; CLI-supervised executor preferred over git2/gix; C3-style approve_once CAS; hard excludes hard-reset/clean/force/rebase/merge/push/argv; zbbody Apache-2.0 conceptual only; Grok App cbbbdf17 porcelain conceptual only; allowlist file only.`
