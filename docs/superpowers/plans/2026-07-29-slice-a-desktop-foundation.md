# Keeper IDE Slice A Desktop Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver a clean Windows desktop IDE foundation that opens local projects, persists them in SQLite, and presents a Grok App-derived project/conversation workbench without running an Agent yet.

**Architecture:** Tauri owns the desktop process, SQLite, filesystem trust boundary, and typed commands. React owns projection and interaction only. The project kernel is independent of Tauri, while the first renderer reuses selected Grok App layout and primitive patterns under the MIT license.

**Tech Stack:** Rust 2024, Tauri 2, rusqlite 0.32 with bundled SQLite, React 19, TypeScript 5.8, Vite 6, pnpm 10, Vitest 3, Playwright.

## Global Constraints

- Repository root is `<workspace>/VaultSystem\keeper-ide`.
- The legacy repository `<workspace>/VaultSystem\keeper` is read-only.
- Stage 1 is an industrial AI IDE; Obsidian and company orchestration are out of scope.
- SQLite is the only operational authority.
- The renderer never opens SQLite, launches processes, or reads secrets.
- Every durable work object is project-scoped.
- The first UI follows Grok App commit `1c3f9c8aa3a42e869f11afb3d075f1b52843aaa9`.
- Adapted Grok App code records its source path and retains the MIT notice.
- Default UI is a neutral light theme; no cinematic effects, particle fields, dashboards, or AI faces.
- TDD is required for domain behavior. Generated configuration is verified by deterministic structure checks.
- Windows is the primary supported development and packaging target.
- No source file introduced by this slice may exceed 500 lines.

---

## File Map

```text
Cargo.toml                              Rust workspace boundary
package.json                            JavaScript command entrypoints
pnpm-workspace.yaml                     JavaScript workspace boundary
LICENSE                                 Keeper IDE Apache-2.0 license
THIRD_PARTY_NOTICES.md                  Imported/adapted source notices
scripts/check-workspace.mjs             Deterministic scaffold check
crates/kernel-project/                  Project domain contracts
crates/storage-sqlite/                  SQLite migrations and repository
apps/desktop/src-tauri/                 Tauri composition root and commands
apps/desktop/src/                       React workbench
apps/desktop/src/features/projects/     Project launcher and switcher
apps/desktop/src/features/sessions/     Empty-state conversation navigation
apps/desktop/src/features/workbench/    Main layout and resource pane
apps/desktop/src/styles/                Grok App-derived tokens and shell CSS
apps/desktop/tests/                     Renderer behavior tests
```

## Task 1: Workspace And Desktop Composition Root

**Files:**
- Create: `Cargo.toml`
- Create: `package.json`
- Create: `pnpm-workspace.yaml`
- Create: `LICENSE`
- Create: `THIRD_PARTY_NOTICES.md`
- Create: `scripts/check-workspace.mjs`
- Create: `apps/desktop/package.json`
- Create: `apps/desktop/index.html`
- Create: `apps/desktop/tsconfig.json`
- Create: `apps/desktop/vite.config.ts`
- Create: `apps/desktop/src-tauri/Cargo.toml`
- Create: `apps/desktop/src-tauri/build.rs`
- Create: `apps/desktop/src-tauri/tauri.conf.json`
- Create: `apps/desktop/src-tauri/capabilities/default.json`
- Create: `apps/desktop/src-tauri/src/main.rs`
- Create: `apps/desktop/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: none.
- Produces: a compilable Tauri composition root; `keeper_ide_desktop::run()`; root commands `pnpm check`, `pnpm typecheck`, `pnpm test`, and `pnpm desktop:dev`.

- [ ] **Step 1: Write the deterministic scaffold check**

```js
// scripts/check-workspace.mjs
import { access, readFile } from "node:fs/promises";

const required = [
  "Cargo.toml",
  "package.json",
  "pnpm-workspace.yaml",
  "apps/desktop/package.json",
  "apps/desktop/src-tauri/tauri.conf.json",
];

for (const file of required) await access(new URL(`../${file}`, import.meta.url));

const rootPackage = JSON.parse(
  await readFile(new URL("../package.json", import.meta.url), "utf8"),
);
if (rootPackage.private !== true) throw new Error("root workspace must be private");
if (rootPackage.packageManager !== "pnpm@10.14.0") {
  throw new Error("pnpm version is not locked");
}
console.log("workspace structure ok");
```

- [ ] **Step 2: Run the check and verify RED**

Run: `node scripts/check-workspace.mjs`

Expected: failure because the root manifests and desktop files do not exist.

- [ ] **Step 3: Create the minimal workspace**

The root Rust manifest must declare only:

```toml
[workspace]
resolver = "2"
members = [
  "apps/desktop/src-tauri",
  "crates/kernel-project",
  "crates/storage-sqlite",
]

[workspace.package]
edition = "2024"
license = "Apache-2.0"
version = "0.1.0"

[workspace.dependencies]
chrono = { version = "0.4", features = ["serde"] }
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
uuid = { version = "1", features = ["v4", "serde"] }
```

The Tauri entrypoint must contain no domain logic:

```rust
fn main() {
    keeper_ide_desktop::run();
}
```

`THIRD_PARTY_NOTICES.md` must record the Grok App repository, inspected commit,
MIT license, and state that adapted files will list their original paths.

- [ ] **Step 4: Verify GREEN**

Run:

```powershell
node scripts/check-workspace.mjs
pnpm install --frozen-lockfile=false
pnpm typecheck
cargo check -p keeper-ide-desktop
```

Expected: all commands exit `0`.

- [ ] **Step 5: Commit**

```powershell
git add Cargo.toml package.json pnpm-workspace.yaml LICENSE THIRD_PARTY_NOTICES.md scripts apps/desktop
git commit -m "build: scaffold Keeper IDE desktop workspace"
```

## Task 2: Project Kernel And SQLite Authority

**Files:**
- Create: `crates/kernel-project/Cargo.toml`
- Create: `crates/kernel-project/src/lib.rs`
- Create: `crates/storage-sqlite/Cargo.toml`
- Create: `crates/storage-sqlite/src/lib.rs`
- Create: `crates/storage-sqlite/src/migrations.rs`
- Create: `crates/storage-sqlite/migrations/0001_projects.sql`
- Create: `crates/storage-sqlite/tests/project_repository.rs`

**Interfaces:**
- Consumes: root Rust workspace from Task 1.
- Produces:
  - `ProjectId(Uuid)`
  - `Project`
  - `CreateProject`
  - `ProjectRepository`
  - `SqliteProjectRepository::open(path)`
  - `SqliteProjectRepository::create(input)`
  - `SqliteProjectRepository::list()`
  - `SqliteProjectRepository::get(id)`

- [ ] **Step 1: Write failing repository tests**

```rust
#[test]
fn creates_and_lists_a_project() {
    let temp = tempfile::tempdir().unwrap();
    let repo = SqliteProjectRepository::open(temp.path().join("keeper.db")).unwrap();
    let created = repo.create(CreateProject {
        name: "Keeper IDE".into(),
        root_path: r"<workspace>/VaultSystem\keeper-ide".into(),
    }).unwrap();

    let projects = repo.list().unwrap();
    assert_eq!(projects, vec![created]);
}

#[test]
fn rejects_the_same_canonical_root_twice() {
    let temp = tempfile::tempdir().unwrap();
    let repo = SqliteProjectRepository::open(temp.path().join("keeper.db")).unwrap();
    let input = CreateProject {
        name: "Keeper IDE".into(),
        root_path: r"<workspace>/VaultSystem\keeper-ide".into(),
    };
    repo.create(input.clone()).unwrap();
    let error = repo.create(input).unwrap_err();
    assert!(matches!(error, ProjectStoreError::DuplicateRoot));
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p keeper-storage-sqlite --test project_repository`

Expected: compile failure because the crates and repository do not exist.

- [ ] **Step 3: Implement the minimal project model**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub root_path: String,
    pub trust: ProjectTrust,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateProject {
    pub name: String,
    pub root_path: String,
}

pub trait ProjectRepository {
    type Error;
    fn create(&self, input: CreateProject) -> Result<Project, Self::Error>;
    fn list(&self) -> Result<Vec<Project>, Self::Error>;
    fn get(&self, id: ProjectId) -> Result<Option<Project>, Self::Error>;
}
```

The migration must create a `projects` table with a unique
`canonical_root_path`, UTC timestamps, and `trust_state` constrained to
`untrusted` or `trusted`.

- [ ] **Step 4: Run focused and crate tests**

Run:

```powershell
cargo test -p keeper-storage-sqlite --test project_repository
cargo test -p keeper-project
cargo test -p keeper-storage-sqlite
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```powershell
git add crates/kernel-project crates/storage-sqlite
git commit -m "feat: add SQLite-backed project kernel"
```

## Task 3: Typed Tauri Project Commands

**Files:**
- Create: `apps/desktop/src-tauri/src/project_commands.rs`
- Create: `apps/desktop/src-tauri/src/app_state.rs`
- Create: `apps/desktop/src-tauri/tests/project_commands.rs`
- Modify: `apps/desktop/src-tauri/src/lib.rs`
- Modify: `apps/desktop/src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: `SqliteProjectRepository` from Task 2.
- Produces:
  - `create_project(name, root_path) -> ProjectDto`
  - `list_projects() -> Vec<ProjectDto>`
  - `select_project(project_id) -> ProjectDto`
  - Tauri managed `AppState`

- [ ] **Step 1: Write command-service tests**

```rust
#[test]
fn selected_project_must_exist() {
    let state = test_state();
    let error = state.select_project(Uuid::new_v4().to_string()).unwrap_err();
    assert_eq!(error.code, "project_not_found");
}

#[test]
fn selection_is_not_persisted_as_global_project_status() {
    let state = test_state();
    let project = state.create_project("A", test_root("a")).unwrap();
    state.select_project(project.id.clone()).unwrap();
    assert_eq!(state.list_projects().unwrap().len(), 1);
}
```

- [ ] **Step 2: Run tests and verify RED**

Run: `cargo test -p keeper-ide-desktop --test project_commands`

Expected: compile failure because `AppState` and command service do not exist.

- [ ] **Step 3: Implement command boundary**

The public error type is allow-listed:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
}
```

The renderer receives project DTOs only. Database paths, SQL errors, and local
diagnostic details remain in host logs.

- [ ] **Step 4: Verify**

Run:

```powershell
cargo test -p keeper-ide-desktop --test project_commands
cargo test -p keeper-ide-desktop
cargo clippy -p keeper-ide-desktop -- -D warnings
```

Expected: all commands exit `0`.

- [ ] **Step 5: Commit**

```powershell
git add apps/desktop/src-tauri
git commit -m "feat: expose typed project desktop commands"
```

## Task 4: Grok App-Derived Workbench UI

**Files:**
- Create: `apps/desktop/src/main.tsx`
- Create: `apps/desktop/src/App.tsx`
- Create: `apps/desktop/src/lib/projectGateway.ts`
- Create: `apps/desktop/src/lib/tauriProjectGateway.ts`
- Create: `apps/desktop/src/lib/mockProjectGateway.ts`
- Create: `apps/desktop/src/features/projects/ProjectSidebar.tsx`
- Create: `apps/desktop/src/features/projects/ProjectLauncher.tsx`
- Create: `apps/desktop/src/features/sessions/ConversationSidebar.tsx`
- Create: `apps/desktop/src/features/workbench/Workbench.tsx`
- Create: `apps/desktop/src/features/workbench/ResourcePane.tsx`
- Create: `apps/desktop/src/components/WindowControls.tsx`
- Create: `apps/desktop/src/styles/tokens.css`
- Create: `apps/desktop/src/styles/workbench.css`
- Create: `apps/desktop/tests/project-workbench.test.tsx`

**Interfaces:**
- Consumes: Task 3 command names and `ProjectDto`.
- Produces: project launcher, project/session sidebar, dominant conversation
  surface, contextual resource pane, and a `ProjectGateway` boundary.

- [ ] **Step 1: Write failing renderer behavior tests**

```tsx
it("keeps the selected project visible above its conversations", async () => {
  render(<App gateway={fixtureGateway()} />);
  await userEvent.click(await screen.findByRole("button", { name: "Keeper IDE" }));
  expect(screen.getByTestId("active-project-name")).toHaveTextContent("Keeper IDE");
  expect(screen.getByRole("navigation", { name: "项目对话" })).toBeVisible();
});

it("does not render provider internals in the empty workbench", async () => {
  render(<App gateway={fixtureGateway()} />);
  expect(screen.queryByText(/routeCode|lease|slotId/)).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run tests and verify RED**

Run: `pnpm --filter @keeper-ide/desktop test -- project-workbench.test.tsx`

Expected: failure because the renderer components do not exist.

- [ ] **Step 3: Port the workbench baseline**

Use these Grok App source areas as the primary reference:

- `src/App.tsx` for shell regions and resize behavior;
- `src/components/VirtualList.tsx` for bounded navigation lists;
- `src/components/WindowControls.tsx` for desktop chrome;
- `src/components/lobe-chat/ConversationThread.tsx` for conversation rhythm;
- `src/styles/tokens.css` for neutral token hierarchy;
- selected layout rules from `src/styles/app.css`.

Do not copy the Grok-specific account, quota, wallpaper, voice, automation,
model menu, or relay surfaces.

The gateway contract is:

```ts
export type ProjectDto = {
  id: string;
  name: string;
  rootPath: string;
  trust: "untrusted" | "trusted";
  createdAt: string;
  updatedAt: string;
};

export interface ProjectGateway {
  listProjects(): Promise<ProjectDto[]>;
  createProject(input: { name: string; rootPath: string }): Promise<ProjectDto>;
  selectProject(projectId: string): Promise<ProjectDto>;
}
```

- [ ] **Step 4: Verify renderer**

Run:

```powershell
pnpm --filter @keeper-ide/desktop typecheck
pnpm --filter @keeper-ide/desktop test
pnpm --filter @keeper-ide/desktop build:ui
```

Expected: all commands exit `0`.

- [ ] **Step 5: Record provenance and commit**

Append every directly adapted file and source path to
`THIRD_PARTY_NOTICES.md`.

```powershell
git add apps/desktop/src apps/desktop/tests THIRD_PARTY_NOTICES.md
git commit -m "feat: add Grok App-based project workbench"
```

## Task 5: Slice A Integration And Desktop Acceptance

**Files:**
- Create: `apps/desktop/tests/e2e/project-launcher.spec.ts`
- Create: `scripts/smoke-desktop.mjs`
- Create: `docs/verification/2026-07-29-slice-a-acceptance.md`
- Modify: `package.json`
- Modify: `apps/desktop/package.json`

**Interfaces:**
- Consumes: Tasks 1-4.
- Produces: one command for the full Slice A gate and measured desktop evidence.

- [ ] **Step 1: Write the end-to-end acceptance test**

```ts
test("creates, selects, and restores a project", async ({ page }) => {
  await page.getByRole("button", { name: "新建项目" }).click();
  await page.getByLabel("项目名称").fill("Keeper IDE");
  await page.getByLabel("项目目录").fill("<workspace>/VaultSystem\\keeper-ide");
  await page.getByRole("button", { name: "创建" }).click();
  await expect(page.getByTestId("active-project-name")).toHaveText("Keeper IDE");
  await page.reload();
  await expect(page.getByTestId("active-project-name")).toHaveText("Keeper IDE");
});
```

- [ ] **Step 2: Run and verify RED**

Run: `pnpm --filter @keeper-ide/desktop test:e2e`

Expected: failure because the E2E harness and persistence wiring are incomplete.

- [ ] **Step 3: Wire the production gateway and smoke harness**

The root gate must run:

```json
{
  "scripts": {
    "check": "node scripts/check-workspace.mjs && pnpm typecheck && pnpm test && cargo test --workspace && cargo clippy --workspace -- -D warnings",
    "desktop:smoke": "node scripts/smoke-desktop.mjs"
  }
}
```

The smoke harness starts the desktop app with a temporary data directory,
waits for the project launcher readiness marker, records startup duration, and
terminates the full process tree.

- [ ] **Step 4: Run the complete gate**

Run:

```powershell
pnpm check
pnpm --filter @keeper-ide/desktop build:ui
pnpm --filter @keeper-ide/desktop test:e2e
pnpm desktop:smoke
cargo build -p keeper-ide-desktop
```

Expected: every command exits `0`; the acceptance report records commands,
counts, startup time, screenshot paths, residuals, and upstream provenance.

- [ ] **Step 5: Commit**

```powershell
git add package.json apps/desktop scripts docs/verification
git commit -m "test: close Keeper IDE desktop foundation slice"
```

