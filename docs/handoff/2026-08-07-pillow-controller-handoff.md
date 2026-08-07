# Pillow 总控完整交接手册

快照时间：2026-08-07  
工作目录：`<repo>`  
分支 / HEAD：`main` / `62149be9eddd2288eb45fd9b8278641cf64c7a5f`  
交接性质：总控职责、实时状态、未完成验收与后续路线的完整移交

> 本文件取代 `docs/handoff/2026-08-01-pillow-controller-handoff.md`。旧文件只适用于
> C3/C4 初期，存在已经过期甚至自相矛盾的状态描述，不得再作为当前真相源。

## 1. 一句话结论

Pillow 仍只推进 Stage 1 工业级本地 AI IDE。C1、C2、C3 已形成接受的
checkpoint；C4 的 governed process 闭环（D3B3）已经由上一任总控接受，
read-only Git Changes 的 Round 1、Round 2A、Round 2B 也已经由上一任总控
亲验接受。Round 3 真实 Tauri/WebView2 Git E2E 已由实际 Grok4 Worker 完成并
留下 12/12 场景通过记录，但**尚未经过总控 Round 4 的源码审查、fresh focused
gate、`pnpm check`、生产构建、完整串行桌面 E2E、视觉检查和 checkpoint
收口**。因此当前准确状态是：

`C4 D3B4 Worker 完成，Controller acceptance pending`。

没有在途 Worker，没有监控任务，没有 Grok/Pillow 残留进程，也没有 1420 端口
监听。暂存区为空。当前大脏工作树属于已接受基础与待验收 D3B4 的累积成果，
不得 reset、restore、clean、覆盖或按旧 HEAD 回退。

## 2. 产品主线与不可改变的顺序

锁定路线：

1. Stage 1：工业级本地 AI IDE。
2. Stage 2：Obsidian 治理知识层。
3. Stage 3：一人公司项目操作系统。

现在只推进 Stage 1。能力阶梯为：

`C1 checkpoint -> C2 项目资源工作台 -> C3 治理型写入/diff/审批 -> C4 受治理进程与只读 Git -> D Codex/OpenCode Runtime 扩展 -> E 工业化 Alpha`

当前不得提前开始：Obsidian、公司运营层、凭据/Key 管理、更多 Runtime、任意
MCP/provider-native 工具、Git 写入、交互式 Shell、远端 Git、发布或打包。

## 3. 当前能力矩阵

| 能力 | 状态 | 当前真实边界 |
| --- | --- | --- |
| A 桌面基础 | Accepted | Tauri/React、项目入口、SQLite 项目权威 |
| B1 对话内核 | Accepted | project-scoped 对话、持久事件、流式运行、取消与恢复 |
| B2 Grok ACP | Accepted | 真实适配器位于 provider-neutral runtime 边界后 |
| B3 governed read | Accepted | 固定 `fs.list@1`、`fs.read@1`、`fs.search@1`，收据与 replay |
| C1 provider-to-B3 | Accepted checkpoint | provider proposal 到受治理只读工具再回所属对话 |
| C2 read-only workbench | Accepted checkpoint | 文件树、搜索、最近文件、预览、真实 Agent 活动、项目隔离、重启恢复 |
| C3 governed write | Accepted checkpoint | 单次有界 UTF-8 create/modify、host diff、approve-once/reject、CAS、原子写入 |
| C4 D1-D3B1 | Controller accepted foundation | 进程协议/策略、监督执行、持久提案、只读 Git runner、native/TS seam、单审批条仲裁 |
| C4 D3B2 | Controller accepted | 对话内进程活动、审批、Terminal 输出上下文面板、项目/对话隔离 |
| C4 D3B3 | Controller accepted | provider `pillow/process_tool` 到 B3 治理进程闭环、真实桌面 process E2E、cleanup race 修复 |
| C4 D3B4 Round 1/2 | Controller accepted | native Git authority、typed bridge、Changes pane、App/Workbench generation/project 隔离 |
| C4 D3B4 Round 3 | Worker complete; controller pending | 真实 Tauri/WebView2 Git E2E 与证据已落地，尚未完成 Round 4 总控验收 |
| D Runtime 扩展 | Not started | C4 checkpoint 前禁止开始 |
| E 工业化 Alpha | Not started | 未来做 packaging、import、安全、性能、诊断 |

## 4. 实时仓库与机器状态

本交接生成前后的实时核对结果：

| 项目 | 当前值 |
| --- | --- |
| Git root | `<repo>` |
| Branch | `main` |
| HEAD | `62149be9eddd2288eb45fd9b8278641cf64c7a5f` |
| HEAD 含义 | `feat(stage1): checkpoint governed IDE through C4 D3AR2` |
| Staging | 空 |
| Worktree | 大脏树，属于用户与各波次累积成果；本手册写入后应为 41 modified + 67 untracked |
| Active Grok Worker | 0 |
| `pillow.exe` | 0 |
| 1420 listener | 0 |
| 最近完成 Worker | 实际 Grok4，C4 D3B4 Round 3 Git E2E |
| Keeper handoff boot | `<session-id>`，只用于记录本次交接 |

上述计数只用于识别漂移。新总控必须重新跑 `git status --short`，不能为了匹配本
快照而回退 live tree。

### 4.1 当前累积工作的大类

当前 HEAD 之后的脏树主要包含：

- D3B1-D3B3 governed process native/service/adapter/renderer/UI/E2E；
- provider process normalization、fake ACP 场景与 roundtrip fixtures；
- single-bar write/process arbitration、conversation-owned process activity、Terminal 输出；
- `desktop-process.mjs` 和 `run-cargo.mjs` 的 Windows 安全清理与自然退出竞态修复；
- D3B4 native Git commands、strict Tauri/TS gateway、Changes pane、App/Workbench 状态隔离；
- D3B4 真实桌面 Git E2E、截图与执行报告；
- 各轮 brief、report、plan、spec 与过期但尚未更新的 README/roadmap 状态文字。

新总控不得仅看 `git diff --stat`：大量新文件仍是 untracked，不会出现在普通
`git diff --stat` 中。必须同时查看 `git status --short`。

## 5. 权威文档阅读顺序

新总控在任何修改、派发或验收前依次阅读：

1. `<workspace>/AGENTS.md`
2. `<repo>\AGENTS.md`
3. `<workspace>/VaultSystem\vault\AGENTS.md`
4. 本交接文件
5. `README.md`（注意其 C4 文字已过期，只作背景）
6. `docs/superpowers/specs/2026-07-29-pillow-first-design.md`
7. `docs/roadmap/2026-07-30-stage1-plan-comparison.md`
8. `docs/roadmap/2026-07-31-stage1-overall-status.md`（状态停在 D3B1，待 checkpoint 修正）
9. `docs/migration/legacy-capability-inventory.md`
10. `docs/references/upstream-baseline.md`
11. `docs/superpowers/specs/2026-08-05-c4-git-changes-design.md`
12. `docs/superpowers/plans/2026-08-05-c4-git-changes-implementation.md`
13. D3B4 的 4 份实现报告与 Round 3 报告（见第 10 节）

规则冲突时采用“更具体、离目标文件更近”的约束：对 `<repo>` 内的项目执行，
`<repo>\AGENTS.md` 的 Keeper record-only 规则优先于根目录/共享 Vault 的通用
context-pack 或治理建议。可以为合规做 Keeper boot 和记录，但不得因此向 Pillow 注入上下文、
执行项目工作或改写项目事实。

## 6. 总控与 Keeper 的职责边界

### 6.1 总控只负责

- 目标、设计约束、协议与阶段边界；
- Worker 任务拆分、文件白名单、输入/输出合同；
- 并行冲突裁决与集成顺序；
- 亲自审查源码和执行验收；
- 维护 README、roadmap、acceptance、checkpoint 和 handoff 真相；
- 必要的 Keeper boot、slot 记录、completion summary。

总控不应承担主体编码。主体开发可交给 Grok/Codex2，但 Worker 自报与进程退出都
不构成接受。

### 6.2 Keeper 只做记录

`<workspace>/VaultSystem\keeper` 是冻结旧资源管理器，不得写入 Pillow IDE 功能。
Pillow 仓库 `AGENTS.md` 明确规定 Keeper 为 record-only：可 boot、查询 runtime、
claim/release slot、记录 completion；不得让 Keeper 生成/改写 prompt、执行项目、
注入 context pack、判断完成或强制 Vault ingest。

历史 Worker 以 `agentRole=worker` boot 时常因缺少 `parentSessionId` 收到 HTTP 400。
未来必须由新总控使用自己 fresh boot 的父 session 正确派生 Worker，或如实记录
boot 失败；绝不允许 Worker 冒充 controller。

### 6.3 Grok 槽位

- 每次启动前按当前 `AGENTS.md` 通过 Keeper claim，结束即 release；
- `grok1` 使用默认 profile；`grok2/3/4` 使用注册 wrapper 语义；
- 不得只设置 `GROK_HOME` 模拟 2-4 号槽位；
- 不读取或输出 auth/token 文件；身份和配额以 Keeper runtime metadata 与注册
  config source 为准；
- 派发后只做一次 liveness 检查，之后由 Worker 发 completion/blocker 信号；
- 不创建每五分钟刷新、前台循环或长篇轮询日志。

## 7. 已由上一任总控亲自接受的最新证据

### 7.1 D3B3 governed process 闭环

上一任总控完成源码审查并亲自运行：

- `pnpm check`：通过；当时 desktop Vitest 323/323，加 Rust workspace/doctest 和
  Clippy `-D warnings`；
- production UI build：通过；
- 完整串行 desktop E2E：9/9；
- `cargo fmt`、`git diff --check`：通过；
- 残留 Pillow/1420 listener：0。

D3B3 的 cleanup final-snapshot race 已由 Grok4 修复后再由总控接受。不要重新引入
“根进程已经自然退出后再按 PID kill”或根据 PID/name 清理 descendants 的逻辑。

### 7.2 D3B4 Round 1

- Grok1：native `GitCommandState` 与 3 个 Tauri command；总控 focused Rust 14/14；
- Grok2：single-source Git DTO 与 `TauriGitGateway`；总控 focused TS 18/18；
- 总控审查确认 renderer 不能传 root/executable/argv/cwd/env，Rust 仅调用固定
  `GitReadExecutor` 模板，项目/仓库/generation 权威在 host。

### 7.3 D3B4 Round 2A

- Grok3：`GitChangesView`、`ResourcePane` contextual mode、wire DTO convergence；
- 总控 focused 43/43、typecheck、scoped diff check 通过；
- pane 优先级：Terminal -> governed write Diff -> Git Changes -> 普通资源/预览；
- 对话仍是视觉中心，没有第二套 dashboard。

### 7.4 D3B4 Round 2B

Grok4 初版被总控拒绝：fresh 测试只有 11/12，且 App 在项目切换首帧缺少
Git model ownership guard。Grok4 窄修后，总控重新审查并亲自运行：

- Git integration 13/13；
- approval arbitration + Git integration 20/20；
- typecheck、scoped diff check：通过；
- render-time guard 要求 `gitScopeRef.current.projectId === activeProject.id`；
- 暂存为空，残留 Grok 进程 0。

这段历史非常重要：不得删除 ownership guard，也不得让一个同时存在 write/process
请求的测试错误假定 write 必然胜出。既有 arbitration 按 decision availability、
scope、expiry、opaque request id 和固定 tie-break 选择唯一审批条。

## 8. 当前唯一未完成事项：D3B4 Round 4 总控验收

### 8.1 Worker 已完成的 Round 3

Grok4 已创建：

- `scripts/lib/c4-git-changes-acceptance.mjs`
- `scripts/wave-c4-git-changes-acceptance.mjs`
- `apps/desktop/tests/e2e/c4-git-changes.spec.ts`
- `apps/desktop/package.json` 中的 `test:e2e:c4:git` 与完整 serial E2E 追加项
- `docs/execution/2026-08-06-wave-c4-d3b4-grok4-real-desktop-git-e2e-report.md`

Worker 最终记录：真实 Tauri/WebView2 + real host Git，12/12 场景通过，focused
E2E 1/1，约 249 秒；typecheck/build/node syntax/scoped diff 通过；Pillow 和 1420
残留为 0；fixture hash manifest 显示 read-only 场景不变。

场景包括：根仓库、嵌套仓库、modified/untracked、rename、conflict、detached
HEAD、text diff、binary/oversized、unsafe config denial、generation drift、项目
切换隔离、重启恢复。

### 8.2 必须保留的证据

忽略目录 `.superpowers/c4-git-acceptance/` 当前包含：

- `c4-git-acceptance-record.json`：最终 pass 记录；
- `hash-manifest.json`：fixture/场景 hash；
- `git-changes-1440x960.png`；
- `git-changes-1024x720.png`；
- `restart-before-1440x960.png`；
- `restart-after-1440x960.png`；
- `c4-git-acceptance-failure.json`：较早失败尝试，错误是 visual proof 时对话区为空。

最后一项不是最终失败。它必须保留为审计链：早期运行失败，之后 Worker 修正
harness 并产生最终 pass record。新总控应比较时间戳和源码，不能只因目录中存在
`failure.json` 就误判当前最终结果，也不能删除它来美化证据。

### 8.3 尚未完成的 Controller 工作

新总控必须独立完成以下工作后才能写 ACCEPT：

1. 审查 Round 3 五个 allowlist 文件的全部源码；
2. 验证 harness 只用 direct Git 做 fixture setup/hash，UI assertion 必须走真实
   `GitCommandState -> GitReadExecutor`；
3. 验证没有 PID/name-wide kill、renderer 注入、`PILLOW_RUNTIME=fake`、用户真实
   repo/profile 污染；
4. fresh focused native/bridge/pane/workbench/Git E2E；
5. fresh `pnpm check`；
6. fresh production build；
7. fresh 完整串行 desktop E2E；
8. fmt、diff、staging、residual 检查；
9. 人工查看 4 张截图，确认对话主导、Changes 紧凑、没有路径/ID/debug 泄漏；
10. 写 acceptance/checkpoint 并同步 README/roadmap 的过期状态。

## 9. 新总控精确接管步骤

### 9.1 Fresh boot 与仓库真相

```powershell
Set-Location <repo>

$body = @{
  project = "pillow"
  task = "pillow-controller-takeover"
  agentName = "<new-controller-name>"
  agentRole = "controller"
} | ConvertTo-Json

Invoke-RestMethod -Method Post `
  -Uri "http://127.0.0.1:7890/api/agent-bridge/boot" `
  -ContentType "application/json" `
  -Body $body

git rev-parse --show-toplevel
git branch --show-current
git rev-parse HEAD
git status --short
git diff --cached --name-only
```

`<new-controller-name>` 是说明性占位符，必须替换为新总控的真实 agent 名称，不得原样提交请求。

预期：root 为 `<repo>`、branch 为 `main`、HEAD 为 `62149be9...`、staging
为空且大脏树仍在。如果 live truth 不同，先查谁改变了它；不得强制恢复到本文快照。

### 9.2 Round 3 低成本门

```powershell
node --check scripts/lib/c4-git-changes-acceptance.mjs
node --check scripts/wave-c4-git-changes-acceptance.mjs

git diff --check -- `
  scripts/lib/c4-git-changes-acceptance.mjs `
  scripts/wave-c4-git-changes-acceptance.mjs `
  apps/desktop/tests/e2e/c4-git-changes.spec.ts `
  apps/desktop/package.json `
  docs/execution/2026-08-06-wave-c4-d3b4-grok4-real-desktop-git-e2e-report.md
```

本交接当日已重跑并通过上述两个 node syntax check 与 scoped diff check，但新总控
仍应在自己的验收 turn 中产生 fresh evidence。

### 9.3 Focused controller gates

按顺序串行运行，避免多个 pnpm/cargo 命令争用 node_modules 或 target：

```powershell
node scripts/run-cargo.mjs test -p pillow-desktop git_commands -- --nocapture

pnpm --filter @pillow/desktop exec vitest run `
  tests/tauri-git-gateway.test.ts `
  tests/c4-execution-primitives.test.tsx `
  tests/resource-pane.test.tsx `
  tests/approval-arbitration.test.ts `
  tests/c4-git-changes-integration.test.tsx `
  --no-cache

pnpm --filter @pillow/desktop typecheck
pnpm --filter @pillow/desktop test:e2e:c4:git
```

不要并行执行 pnpm install/test；历史上并发命令曾产生 `.bin/playwright-core`
瞬时 chmod/ENOENT 噪声。focused Git E2E 正常约 4-5 分钟，应前台等待自然退出，
不要加 10 分钟全局杀死超时。

### 9.4 Aggregate gates

Focused 全绿且源码审查无 Critical/Important 后再运行：

```powershell
pnpm check
pnpm --filter @pillow/desktop build
pnpm --filter @pillow/desktop test:e2e
node scripts/run-cargo.mjs fmt --all -- --check
git diff --check
git diff --cached --name-only
```

`test:e2e` 已追加 Git spec，必须 `--test-concurrency=1` 串行；不要拆成并行桌面
进程。完整套件会很慢，以终端最终 exit code 与每个 spec 结果为准，不根据“进程仍
存在”提前宣告 pass/fail。

### 9.5 Fresh residual 与视觉检查

只读检查：

```powershell
Get-CimInstance Win32_Process | Where-Object Name -ieq "pillow.exe"
Get-NetTCPConnection -State Listen -LocalPort 1420 -ErrorAction SilentlyContinue
```

如果有残留，只能通过本次 launcher 的稳定 handle 与 ownership evidence 清理。
不得 `taskkill /T`、不得按数字 PID 递归 kill、不得按进程名全杀、不得杀不明 Node/
Cargo/rustc/WebView2。

使用图片查看工具人工检查 4 张 `.superpowers/c4-git-acceptance/*.png`。确认：

- 1440x960 与 1024x720 都由对话区主导；
- Changes 是可收起上下文面板，不是 dashboard；
- 仓库选择、状态、一个 diff 使用渐进披露；
- 无盘符根路径、临时目录、UUID、session/run/proposal id、argv/cwd/env、token、
  raw RPC 或 debug metadata；
- 重启前后没有旧 generation 混入。

## 10. 关键文件地图

### 10.1 D3B4 设计与计划

- `docs/superpowers/specs/2026-08-05-c4-git-changes-design.md`
- `docs/superpowers/plans/2026-08-05-c4-git-changes-implementation.md`

### 10.2 Round 1-3 报告

- `docs/execution/2026-08-05-wave-c4-d3b4-grok1-native-git-report.md`
- `docs/execution/2026-08-05-wave-c4-d3b4-grok2-git-bridge-report.md`
- `docs/execution/2026-08-05-wave-c4-d3b4-grok3-git-pane-report.md`
- `docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-report.md`
- `docs/execution/2026-08-06-wave-c4-d3b4-grok4-real-desktop-git-e2e-report.md`

### 10.3 Git product seams

- Native：`apps/desktop/src-tauri/src/git_commands.rs`
- Native tests：`apps/desktop/src-tauri/src/git_commands_tests.rs`
- Wire DTO：`apps/desktop/src/lib/gitGateway.ts`
- Tauri bridge：`apps/desktop/src/lib/tauriGitGateway.ts`
- App state：`apps/desktop/src/App.tsx`
- Workbench：`apps/desktop/src/features/workbench/Workbench.tsx`
- Context pane：`apps/desktop/src/features/workbench/ResourcePane.tsx`
- View：`apps/desktop/src/features/execution/GitChangesView.tsx`
- Integration tests：`apps/desktop/tests/c4-git-changes-integration.test.tsx`
- Real E2E：`scripts/wave-c4-git-changes-acceptance.mjs`

### 10.4 Process 与安全底座

- `scripts/lib/desktop-process.mjs`
- `scripts/lib/desktop-process-safety.test.mjs`
- `scripts/run-cargo.mjs`
- `scripts/wave-c4-governed-process-acceptance.mjs`
- `docs/execution/2026-08-05-wave-c4-d3b3-grok4-run-cargo-cleanup-race-report.md`

## 11. Round 3 源码审查的重点风险

新总控不要仅复跑测试，至少审查以下问题：

1. 65 KB orchestrator 是否只清理自身 launcher-owned 进程；
2. fixture direct Git 是否严格限制在 setup/hash，UI proof 是否全走 Tauri；
3. hash manifest 的 `equal: true` 是否排除了 intentional setup change，未把读操作的
   真实 mutation 掩盖；
4. `vendor/` gitignore 作为 nested-repo portable substitute 是否符合设计，而不是
   避开产品缺陷；
5. 未 gitignore 的 nested worktree 可能导致 root porcelain trailing-slash 路径触发
   `git_path_rejected`，这是 Worker 报告中的残余产品风险，接受前要明确记录；
6. unsafe config denial 可能表现为 `git_repository_not_found` 或 `git_unavailable`，
   必须固定且 path-free，不能泄漏 host 错误；
7. 失败证据与最终 pass 证据是否来自不同时间的真实运行；
8. package `test:e2e` 是否只追加新 spec 且仍保持 serial；
9. 截图 privacy assertion 是否覆盖 body text、HTML 与可见 UI，而非只检查文件名；
10. normal restart 是否确实重建 native generation，而非复用 renderer cache。

如发现问题，先分类 product/harness/host，再派一个窄修 Worker。不得由总控顺手
大改主体代码，也不得为了让断言通过而降低 authority/privacy/cleanup 标准。

## 12. 文档债务与 checkpoint 收口

`README.md` 和 `docs/roadmap/2026-07-31-stage1-overall-status.md` 当前仍把
D3B2 描述为下一步，已明显过期。只有在 Round 4 全绿后才更新为：

- D3B2 process workbench accepted；
- D3B3 provider/process real desktop loop accepted；
- D3B4 read-only Git Changes accepted；
- C4 当前 checkpoint 到哪里；
- 交互式 shell 与 Git mutation 仍未开始；
- 下一阶段是 C4 后续边界或 D Runtime 扩展，必须另做设计授权。

建议新建：

- `docs/verification/2026-08-07-wave-c4-d3b3-d3b4-acceptance.md`
- `docs/verification/2026-08-07-wave-c4-checkpoint.md`

Acceptance 必须记录 fresh 命令、exit code、测试数量、桌面 E2E 数量/耗时、截图、
残留进程、HEAD、staging 和未解决风险。不得把 Worker report 复制成 Controller
acceptance。

## 13. Stage/commit 规则

当前 staging 为空。虽然历史上用户曾允许“适当清理和提交”，本交接不把那句话解释
成对这个 107+ 文件大 checkpoint 的无限期授权。Round 4 全绿并写完 acceptance/
checkpoint 后，向用户汇报拟提交范围、文档修正和残余风险，再取得当前明确授权后
才能 stage/commit。

禁止：

- `git reset --hard`、`git clean`、`git checkout --`、广泛 `restore`；
- 只提交 Round 3 而遗漏其依赖的 D3B1-D3B4 未跟踪文件；
- 把 `.superpowers` 截图/临时 fixture 纳入提交；
- 提交 token、auth、native temp path、session/log；
- 在没有 fresh aggregate gate 的情况下用 Worker self-report 做 checkpoint。

## 14. Worker 派发模板

如果 Round 4 发现问题，Worker brief 必须包含：

- fresh Keeper boot/parent session 与精确 slot；
- 一句话说明该任务在 C4 D3B4 的位置；
- 明确读入的 spec/report；
- 不重叠的 exact write allowlist；
- 禁止路径与不可降低的 authority/privacy/cleanup 约束；
- RED/GREEN 或复现命令；
- focused tests、typecheck/build/diff/staging 命令；
- report 路径；
- changed files、测试、残余风险、staging、residual、completion/blocker 信号。

优先让实际 Grok 做开发；Codex2 可承担协议级高风险修复。新总控始终是最终 reviewer
与 acceptance owner。不要把内部子代理命名为 `grok4` 或 `codex2` 冒充真实账号
CLI。

## 15. 交接完成状态

- 当前无在途任务；
- Round 3 Worker 已完成，不需要重新派同一实现任务；
- Round 4 Controller acceptance 尚未开始；
- 没有 stage/commit；
- 当前低成本 live gate：两个 Round 3 Node syntax check 通过，scoped diff check
  通过，Pillow/Grok/1420 残留均为 0；
- 新总控的第一项实质工作是 Round 3 源码审查与 fresh focused E2E，而不是规划新
  功能、清理工作树或进入 Stage 2/D；
- 只有 Round 4 全绿、acceptance/checkpoint 文档完成并获得用户明确授权后，才可
  整理提交并规划下一能力边界。
