# Permission Rules UI — implementation report

Date: 2026-08-10  
Worker: permission-ui (render layer only)  
Work directory: `<repo>`  
`CARGO_TARGET_DIR`: `<repo>/target-permui`

## Result

**PERMISSION-UI-COMPLETE** for the renderer slice.

Worker gate: **BLOCKED-BY-CONCURRENT** on `cargo-fmt` only (see § Commands). Concurrent backend Worker has unformatted Rust under `crates/storage-sqlite/**` outside this whitelist. No attempt was made to format or edit those files.

## Contract followed (no field invention)

Tauri commands (via new gateway):

- `list_permission_rules(scope, projectId?)`
- `add_permission_rule(scope, projectId?, prefix, pattern, action)`
- `delete_permission_rule(ruleId)`

DTO field names kept as specified:

- `PermissionRuleView`: `rule_id`, `scope`, `project_id?`, `prefix`, `pattern`, `action`, `created_at`
- Pending proposals: `rule_subject` (read via `readRuleSubject`, with portable fallbacks)
- Receipt / events: `auto_approved_by_rule?: { prefix, pattern, action }`

Project + allow is blocked in UI **before** submit (`isProjectAllowForbidden` + disabled select + visible copy).

## Three UI blocks

### 3.1 Settings — rule list

- `PermissionRulesSection` mounted at the bottom of `SettingsPanel`.
- Global list and project deny list shown **separately**.
- Add form: scope / prefix / pattern / action; delete per row.
- When scope is `project`: action forced to `deny`, allow option disabled, submit cannot send `project`+`allow`.

### 3.2 Approval bar — always allow / deny

- `WriteApprovalBar` and `CommandApprovalBar` add **总是允许此模式** / **总是拒绝此模式** when a permission gateway is available.
- Opens `AlwaysRuleDialog` which always shows:
  - Editable **exact pattern** input (prefilled from `rule_subject` via `suggestPatternFromSubject`)
  - Exact write preview: `将写入：allow process:cargo test *`
  - Word-boundary note (see copy below)
- Allow defaults to **global** scope; deny defaults to **project** when a project is open.
- Switching dialog action to allow while on project forces global (cannot submit project+allow).

### 3.3 Timeline — auto-approve must be visible

- Process: `process_proposal_decided` payload may carry `auto_approved_by_rule`; `buildProcessActivityModels` projects it; `ConversationTimeline` paints a chip on the command activity summary (and detail meta).
- Write: `writeOutcomesFromEvents` / projection / `AgentActivityDisclosure` paint the same chip on write outcome rows.
- Labels:
  - Allow path: **已由规则自动批准**
  - Deny path: **已由规则拒绝**
- Chip summary example: `allow process:cargo *`

## Word-boundary copy (exact)

```
模式按前缀/通配匹配，没有词边界：例如「cargo」会同时命中「cargotest」。若只想匹配该程序及其参数，请使用「cargo *」这类带空格与通配符的形态。
```

Constant: `PERMISSION_WORD_BOUNDARY_NOTE` in `features/permissions/permissionPattern.ts`.  
Shown in Settings add form and Always-rule dialog (`data-testid` ends with `word-boundary-note`).

Suggested process patterns:

- `cargo` → `cargo *`
- `cargo test -p kernel-policy` → `cargo test *`

## Six required tests

File: `apps/desktop/tests/permission-rules-ui.test.tsx` (10 cases total).

| # | Requirement | How covered |
| --- | --- | --- |
| 1 | Rule list render + delete | Renders global + project rows; delete removes global; project remains |
| 2 | Add form validation | Empty pattern → submit disabled; valid pattern adds row |
| 3 | **project+allow UI-disabled (negative)** | Scope=project → action select **disabled**, allow option **disabled**, value forced to deny, `permission-rules-project-allow-blocked` visible **before** submit, gateway only called with `deny`; pure `isProjectAllowForbidden` + mock reject on forced allow |
| 4 | Always-allow exact pattern + word-boundary | Dialog shows suggested exact pattern + `将写入：…` + full note mentioning `cargotest` / `cargo *` |
| 5 | Timeline auto-approve visible + rule | Process chip + write-outcome chip with label and `allow process:cargo *` / `allow edit:src/**/*.rs` |
| 6 | Dual-skin body contrast ≥ 4.5:1 | Numeric WCAG ratios on tokens.css palette for dark/light primary+secondary on elevated/main |

Related regression pack (66 tests): permission-rules-ui + write-approval-bar + settings-panel + c4-execution-primitives + visual-shell-contract — all green.

## Dual-skin contrast (measured)

Against `tokens.css`:

| Pair | Ratio gate |
| --- | --- |
| Dark `#faf9f7` on `#2c2c36` / `#1e1e24` | ≥ 4.5 |
| Dark secondary `#c7c7ce` on elevated | ≥ 4.5 |
| Light `#3e3e46` on `#ffffff` / `#eeebe6` | ≥ 4.5 |
| Light secondary `#5c5c66` on elevated/main | ≥ 4.5 |

Permission surfaces use `--text-primary` / `--text-secondary` on `--bg-elevated` / content cards — no new low-contrast colors.

## testid / a11y / geometry

- **No existing `data-testid` removed or renamed.**
- Existing approval bar accessible names unchanged (批准一次 / 拒绝 / 查看差异|命令).
- New testids only (permission-rules-*, always-rule-*, *-auto-rule*).
- Shell layout geometry not altered (Settings section is inside the existing modal scroll; approval bar actions wrap with existing flex; no sidebar/rail changes).
- `visual-shell-contract.test.tsx` still passes (assertions only strengthened by suite green, not weakened).

## Backend not ready — test doubles used

- New `MockPermissionRulesGateway` + `TauriPermissionRulesGateway` in `apps/desktop/src/lib/permissionRulesGateway.ts` (same pattern as `settingsGateway` Mock/Tauri split).
- Settings / Workbench default to Tauri when `__TAURI*` is present, else Mock.
- **Did not** modify existing write/exec Tauri parsers (strict key allowlists). When backend wires `rule_subject` through host list DTOs, integration may need a follow-up parse allow-list update by the backend/integration batch — UI already reads `rule_subject` when present and falls back to path / program+argv for suggestions.

## Commands / exit codes

| Command | Exit |
| --- | --- |
| `export CARGO_TARGET_DIR=<repo>/target-permui` | 0 (session) |
| `corepack pnpm exec vitest run tests/permission-rules-ui.test.tsx` | **0** (10/10) |
| Related unit pack (5 files, 66 tests) | **0** (tests green; PowerShell may surface stderr noise from Tauri window mock) |
| `corepack pnpm --filter @pillow/desktop build` | **0** |
| `node scripts/run-worker-gate.mjs` | **1** — failed at `cargo-fmt` on concurrent Worker files under `crates/storage-sqlite/**` only |

## Files touched (whitelist)

**Added**

- `apps/desktop/src/lib/permissionRulesGateway.ts`
- `apps/desktop/src/features/permissions/permissionPattern.ts`
- `apps/desktop/src/features/permissions/AlwaysRuleDialog.tsx`
- `apps/desktop/src/features/permissions/PermissionRulesSection.tsx`
- `apps/desktop/src/styles/permission-rules.css`
- `apps/desktop/tests/permission-rules-ui.test.tsx`
- `docs/execution/2026-08-10-permission-ui-report.md`

**Modified (features only)**

- Settings panel, Write/Command approval bars, Workbench (default gateway)
- process activity model, conversation timeline, agent activity types/projection/fromTimeline/disclosure

## Not touched

- `apps/desktop/src-tauri/**`, `crates/**`, `scripts/**`, `tests/fixtures/**`
- `docs/**` except this report
- `apps/desktop/src/App.tsx`, `main.tsx` (outside write whitelist; Workbench/Settings self-default gateways)
- `brand-mark.css`
- No git add/commit/stash/reset/clean/checkout
- No desktop E2E
- No new runtime deps, network fonts, image backgrounds, telemetry
- No auth/token file reads

## Residual risks

1. **Host wire parse:** existing `tauriWriteApprovalGateway` / `tauriExecGateway` strict key sets will reject unknown `rule_subject` / `auto_approved_by_rule` until the backend/integration Worker widens allow-lists. Timeline path for process events already accepts extra payload fields (no strict strip). Write outcomes depend on runtime event payloads, not the write gateway list parser.
2. **Two Mock instances:** Settings and Workbench each construct a Mock when no prop is injected (unit tests inject one fixture; production Tauri both hit the same host store).
3. **Gate fmt noise:** concurrent storage Worker must fmt before sole gate is green.
4. **Always-rule does not auto-decide the current pending proposal** — it only persists a rule for future host evaluation (matches design: host evaluates on create/pending; bar still has 批准一次).

## Status line

`PERMISSION-UI-COMPLETE` (render slice).  
Gate: `BLOCKED-BY-CONCURRENT` on `cargo-fmt` for `crates/storage-sqlite/**` owned by parallel backend Worker.
