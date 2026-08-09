# Pillow — 公开前清理审计（只读）

| 字段 | 值 |
| --- | --- |
| Date | 2026-08-09 |
| Mode | **read-only**（本文件为任务唯一写入） |
| Corpus | 工作树 `HEAD` = `7a00214`；`git ls-files` **782** 路径 |
| History | `main` **134** commits；作者全程 `Vault Keeper <keeper.local@localhost>` |
| Remote（观察） | `origin` → GitHub private 仓库（org/user 级路径存在，非密钥） |
| LICENSE | Apache-2.0（已存在） |
| 禁止动作 | 未改任何业务文件；未跑 cargo/pnpm/门/测试；未做任何 git 写操作 |

**结论摘要（处置建议，非历史压缩决定）**

- **工作树未发现真实凭据**（`ghp_` / 私钥 PEM / 长 `xai-` key 等命中落在扫描器自检样本或公开上游包名误撞）。
- **工作树无本机 Windows 用户名路径**（`Users\<HOST_USER>` 已在历史中被 scrub；工作树 handoff 现用 `<USER>` 占位）。
- **对外开源的主风险是“私有运维面”与“误导性 agent 入口”**，不是密钥泄漏：
  1. 根入口 `CLAUDE.md` / `AGENTS.md` / `.cursor/rules/_vault-shared.mdc` 要求连接本地 Keeper Bridge、读机外 vault；
  2. `docs/handoff/*` 含完整派单/claim-release 手册、槽位池语义、另一项目名、账号档位叙事；
  3. 大量 `docs/execution/*brief*` 把 `grokN`/`codexN`、Keeper boot、机内绝对路径当作执行契约；
  4. 本机盘符布局 `<DRIVE>:\new\...` 与 sibling `VaultSystem` / `zbbody` 在 docs/tests 中广泛出现；
  5. **历史三笔**仍含真实 `Users\<HOST_USER>` 字面量（见 §4）。

信号：`PRE-OSS-AUDIT-COMPLETE`

---

## 1. 方法与范围

### 1.1 做了什么

| 步骤 | 方法 |
| --- | --- |
| 跟踪清单 | `git ls-files`（782） |
| 内容扫描 | `git grep` 于工作树：VaultSystem、绝对盘符、`127.0.0.1:7890`、agent-bridge / claim-release、槽位名、auth.json、常见密钥形、邮箱 |
| 入口文件 | 通读 `CLAUDE.md`、`AGENTS.md`、`README.md`、`.cursor/rules/_vault-shared.mdc`、`.superpowers/sdd/*`（当前跟踪 2 份）、`docs/handoff/*`、关键 ADR |
| docs 分类 | 按目录 + 文件名角色（brief/report/verdict/…）+ 内容抽样 |
| 历史 | `git log -S` 用户路径；逐 commit 看 diff 落点；`.superpowers` 历史增删 |

### 1.2 明确**未**审到的范围

| 未覆盖 | 原因 |
| --- | --- |
| 未跟踪文件 / 本地 dirty 内容（除观察到 `Cargo.lock` 已改） | 只审计 `git ls-files` |
| 全部 134 次提交的逐 blob 二进制/大 diff | 用户路径与 `.superpowers` 用 pickaxe + 已知 commit 穷尽；**未**对每个 commit 全文扫密钥 |
| `git notes` / 反射 refs / GitHub Actions secrets | 未见 workflow 深度审计；无 `.github/workflows` 专项逐文件读 |
| 运行时 DB、`.keeper/`、真实 `auth.json`、本机 Grok profile | 禁止读打印 auth/token；且 `.gitignore` 已忽略 |
| 第三方 upstream 克隆（`VaultSystem/references/grok-app`、`zbbody`） | 不在本仓库跟踪树内 |
| 子模块 / LFS 指针完整性 | 未见 submodule 入口专项 |
| 图像 OCR / 截图内嵌文字 | 跟踪了 `artifacts/grok-shell-prototype/*.png` 与 Tauri icons；**未** OCR |
| 邮件/issue/PR 讨论、GitHub 仓库 settings | 仓库外 |
| 依赖树供应链（lockfile CVE） | 超出“不该公开内容”范围 |

### 1.3 占位符约定（本报告**不**复述完整本机路径）

| 占位符 | 含义 |
| --- | --- |
| `<HOST_USER>` | 历史中出现过的本机 Windows 用户目录名 |
| `<DRIVE>:\new\...` | 开发机固定布局（pillow / VaultSystem / zbbody 同盘兄弟目录） |
| `<BRIDGE>` | `http://127.0.0.1:7890`（本地 Keeper Bridge） |
| `<workspace>` | 文档中已用的可移植写法（仍假设 sibling `VaultSystem`） |

---

## 2. §2.1 全量跟踪文件扫描结果

扫描按**问题类别**组织。每条：**路径 · 行号 · 类别 · 建议处置**。

高优先与入口文件给**全行号**；docs 中同类问题给**完整文件列表 + 代表行**，避免 100+ 行重复表淹没信号。

### 2.1.0 无问题或可公开保留（摘要）

| 区域 | 判定 |
| --- | --- |
| 产品源码主体（`apps/`、`crates/` 业务逻辑） | 大体可公开；crate 仍名 `keeper-*` 是**命名过渡**（ADR-0002 已说明），非密钥 |
| `scripts/check-secrets.mjs` 中 `sample:` 假密钥 / 假路径 | **自检样本**；保留（改样本用户名即可，见历史） |
| 测试中 `Alice` / `person@example.com` / `fixture@pillow.local` | 合成夹具，保留 |
| `LICENSE` / `THIRD_PARTY_NOTICES.md` | 应保留 |
| `artifacts/grok-shell-prototype/*.png` | 验收截图；无密钥迹象（未 OCR） |
| `docs/verification/*` 多数验收叙事 | **高对外价值**；需 scrub 绝对路径与 Keeper boot 旁白 |
| `docs/decisions/ADR-*`、`docs/superpowers/specs/*` 设计正文 | **高对外价值**；路径字面量需改写 |
| `docs/references/*` 上游 pin | 可公开 |

---

### 2.1.A 入口与 IDE 规则（最高优先级 · 误导外部贡献者）

| 路径 | 行号 | 类别 | 问题 | 建议处置 |
| --- | --- | --- | --- | --- |
| `CLAUDE.md` | 5–10 | 误导性指令 / 私有系统 | 要求 substantial work 前 `POST <BRIDGE>/api/agent-bridge/boot` | **拆分**：公开版删除或改为“可选本地编排”；本地版 gitignore |
| `CLAUDE.md` | 12–14 | 私有系统引用 | 绝对路径指向机外 `VaultSystem\vault\AGENTS.md` 与 `keeper` | **改写**为抽象“legacy resource manager（可选、非本仓库）”或移本地 |
| `AGENTS.md` | 1–8 | 内部运维叙事 | Keeper record-only、claim/release 会话模型 | **拆分**：公开 `AGENTS.md` 只留工程规则；运维段移 `AGENTS.local.md` |
| `AGENTS.md` | 27–32 | 私有系统 / 边界 | `<workspace>\VaultSystem\keeper` 只读假设 | **改写**：说明 Pillow 自洽；legacy 为可选外部系统 |
| `AGENTS.md` | 47–60 | 本机环境假设 | `grok1–4` 槽位、`GROK_HOME` 陷阱、`auth.json` 路由污染、Keeper wrapper | **移到私有**或 `docs/private/`（gitignore）；公开仅保留“多账号隔离原则”抽象句 |
| `README.md` | 11–16 | 私有系统引用 | 把 legacy Keeper 路径写成产品依赖叙事 | **改写**：开源读者应能从本仓库独立构建理解 Stage 1 |
| `README.md` | 18–79 | 内部运维叙事（轻度） | 长篇 wave 验收流水账 + 内部文件名 | **改写**：对外 README 收敛到“是什么 / 如何构建 / 架构图 / 链接 ADR”；详情链到 `docs/` |
| `.cursor/rules/_vault-shared.mdc` | 2, 7, 15 | 私有系统 + 误导 | `alwaysApply: true`，强制 QUERY/INGEST vault，绝对路径机外规则 | **删除跟踪**或移本地 cursor 规则；**勿**让外部 Cursor 用户默认命中 |
| `.superpowers/sdd/grok-shell-prototype-report.md` | 全文 | 内部执行报告 | 原型实现报告；已在 `.gitignore` 写 `/.superpowers/` 但仍被跟踪 | **停止跟踪**（`git rm --cached`，总控执行）；内容可并入 `docs/execution` 或丢弃 |
| `.superpowers/sdd/wave-b1-ui-report.md` | 4 | 本机环境 | Worktree 绝对路径 `<DRIVE>:\new\pillow-worktrees\...` | 同上 **停止跟踪** |

---

### 2.1.B Keeper Bridge / 本地 HTTP 运维面

| 路径 | 行号 | 类别 | 问题 | 建议处置 |
| --- | --- | --- | --- | --- |
| `CLAUDE.md` | 8 | 误导性指令 | boot URL + JSON body 契约 | 见 2.1.A 拆分 |
| `docs/handoff/2026-08-01-pillow-controller-handoff.md` | 203 | 内部运维 | PowerShell boot 示例 | **移到私有仓库**或改写为“控制器内部 runbook（不随公开树）” |
| `docs/handoff/2026-08-07-pillow-controller-handoff.md` | 268 | 内部运维 | 同上 | 同上 |
| `docs/handoff/2026-08-08-pillow-controller-handoff.md` | 465–478 | 内部运维 | `agent-runner/capability`、`grok-cli/claim|release`、`X-Keeper-Runner-Capability` 完整 curl | **移到私有**（敏感度最高的对外运维说明书之一） |
| `docs/execution/2026-08-01-wave-c3f-project-isolation-fix-brief.md` | 7, 91 | 误导 / 运维 | brief 强制 boot + 完成回写 bridge | 公开保留报告价值则 **改写** 去掉强制 bridge；或归 internal |
| `docs/execution/2026-08-06-wave-c4-d3b4-grok4-real-desktop-git-e2e-report.md` | 13–15 | 运维旁白 | boot 失败 `parentSessionId` | **改写**脚注或删除 boot 段（不损 E2E 证据） |
| `docs/execution/2026-08-08-dual-theme-report.md` | 76–78, 200 | 运维旁白 | Bridge 400 | 同上 |
| `docs/execution/2026-08-08-upstream-grokbuild-audit.md` | 23 | 运维旁白 | Bridge 400 | 同上 |
| 大量 `docs/execution/*brief*.md` / `*report*.md` | 见下表模式 | 运维 | `parentSessionId=<keeper-session>`、worker boot 400 叙事 | **批量改写**模板句 或 接受为历史证据并加 `docs/README` 说明“历史派单契约，非贡献者必读” |

**Bridge / claim 叙事高频模式（非穷尽每一行，文件可 `git grep parentSessionId` 复现）**  
代表文件（均含 boot / parentSession 约束）：

- `docs/execution/2026-08-01-wave-c3a-protocol-policy-brief.md:12`
- `docs/execution/2026-08-01-wave-c3b-storage-ledger-brief.md:17`
- `docs/execution/2026-08-02-wave-c4-d2a-codex2-process-executor-development-brief.md:9`
- `docs/execution/2026-08-02-wave-c4-d2a-grok1-exec-ledger-development-brief.md:8`
- `docs/execution/2026-08-03-wave-c4-d2b-grok2-governed-process-gateway-brief.md:8`（及同波次多份 brief）
- `docs/execution/2026-08-05-wave-c4-d3b4-grok1-native-git-report.md:11`
- `docs/execution/2026-08-05-wave-c4-d3b4-grok2-git-bridge-report.md:11`
- `docs/research/2026-08-01-c3-governed-write-security-research.md:9`
- `docs/research/2026-08-01-c3-grok-app-interaction-research.md:10`

---

### 2.1.C 私有系统路径：`VaultSystem` / vault wiki / sibling 仓库

#### C1. 根与规则（绝对路径字面量）

| 路径 | 行号 | 类别 | 建议处置 |
| --- | --- | --- | --- |
| `CLAUDE.md` | 12–13 | 私有系统 | 拆分/改写 |
| `.cursor/rules/_vault-shared.mdc` | 2, 7, 15 | 私有系统 | 删除跟踪或本地化 |
| `AGENTS.md` | 27 | 私有系统（可移植写法） | 改写 |
| `README.md` | 11 | 私有系统（可移植写法） | 改写 |

#### C2. 产品测试夹具（绝对路径）

| 路径 | 行号 | 类别 | 建议处置 |
| --- | --- | --- | --- |
| `apps/desktop/tests/project-workbench.test.tsx` | 28 | 本机布局假设 | **改写**为合成路径（如 `C:\\Users\\Alice\\proj` 或 tmp） |
| `apps/desktop/tests/tauri-project-gateway.test.ts` | 13 | 同上 | 同上 |
| `apps/desktop/tests/agent-activity-disclosure.test.tsx` | 269, 309, 323 | 隐私测试用绝对路径 | **保留意图**（测不泄漏）；路径可改为通用合成绝对路径 |
| `apps/desktop/tests/agent-activity-timeline-integration.test.tsx` | 452 | 同上 | 同上 |
| `apps/desktop/tests/tauri-git-gateway.test.ts` | 454 | 同上 | 同上 |

#### C3. docs 中 `VaultSystem` / 绝对 `<DRIVE>:\new\...`

**规模**：`git grep VaultSystem` 命中约 **85+ 行 / 61+ docs 文件**；`<workspace>` + `<workspace>` 合计约 **121 行 / ~60 文件**。

**建议处置总则**：公开树内一律改为 `<repo>` / “legacy Keeper（out of tree）” / 合成路径；**不要**保留真实盘符布局。

| 路径 | 代表行号 | 类别 | 建议处置 |
| --- | --- | --- | --- |
| `docs/decisions/ADR-0001-clean-successor.md` | 18 | 本机路径 | **改写**（ADR 价值高） |
| `docs/decisions/ADR-0002-pillow-identity-boundary.md` | 9, 17, 19 | 本机路径 + legacy | **改写**路径，保留决策正文 |
| `docs/migration/legacy-capability-inventory.md` | 5 | 私有源路径 | **改写**为 “legacy Keeper tree（not published）” |
| `docs/handoff/2026-08-07-pillow-controller-handoff.md` | 4, 67, 101–103, 115–116, 136, 258, 281 | 运维 + 绝对路径 + 根 `AGENTS.md` 在 pillow 之外 | **移到私有**优先 |
| `docs/handoff/2026-08-08-pillow-controller-handoff.md` | 3, 410, 439, 493, 503, 516, 523 | 运维 + **vault wiki** 绝对路径 + cwd | **移到私有**优先 |
| `docs/handoff/2026-08-01-pillow-controller-handoff.md` | 39, 42, 216, 316 | 运维 + references 路径 | 移私有或大改写 |
| `docs/research/2026-07-30-grok-adapter-boundary.md` | 13, 638–640 | 私有源文件路径 | **改写**为逻辑模块名 |
| `docs/research/2026-07-30-grok-acp-protocol.md` | 1056 | 同上 | 改写 |
| `docs/research/2026-07-30-grok-security-acceptance.md` | 9, 28 | worktree + legacy path | 改写 |
| `docs/research/2026-07-30-zbbody-reference-analysis.md` | 5, 9, 88 | 本机路径 + **Gitee 上游 URL** | 改写路径；上游 URL 若为第三方公开仓可保留并核对 license |
| `docs/superpowers/plans/2026-07-29-slice-a-desktop-foundation.md` | 13–14, 194, 207, 467 | 计划中的绝对路径与 fill 路径 | 改写 |
| `docs/superpowers/plans/2026-08-05-c4-d3b3-provider-process-roundtrip.md` | 13 | 绝对路径 | 改写 |
| `docs/superpowers/plans/2026-08-05-c4-d3b3-real-desktop-process-acceptance.md` | 13 | 绝对路径 | 改写 |
| `docs/superpowers/plans/2026-08-01-c3-final-acceptance-checkpoint.md` | 84 | 期望 root 绝对路径 | 改写 |
| `docs/superpowers/specs/2026-08-08-settings-credentials-design.md` | 10 | work directory | 改写 |
| `docs/verification/2026-07-29-slice-a-acceptance.md` | 40, 186 | 绝对路径 | 改写 |
| `docs/verification/2026-07-30-slice-b1-acceptance.md` | 5, 26 | worktree 绝对路径 | 改写 |
| `docs/verification/2026-07-30-wave-b3-foundation-acceptance.md` | 51, 117 | zbbody + 绝对路径 | 改写 |
| `docs/verification/2026-08-07-wave-c4-checkpoint.md` | 171 | 绝对路径脚注 | 改写 |

**含 `<workspace>` 或 `<workspace>` 的跟踪文件完整清单（路径级；行数合计 ~121）**

```
.cursor/rules/_vault-shared.mdc
.superpowers/sdd/wave-b1-ui-report.md
CLAUDE.md
docs/decisions/ADR-0001-clean-successor.md
docs/decisions/ADR-0002-pillow-identity-boundary.md
docs/execution/2026-07-30-wave-b3-action-brief.md
docs/execution/2026-08-04-wave-c4-d3b1-codex2-native-process-bridge-brief.md
docs/execution/2026-08-04-wave-c4-d3b1-codex2-portable-cwd-repair-brief.md
docs/execution/2026-08-04-wave-c4-d3b1-grok4-approval-arbitration-brief.md
docs/execution/2026-08-04-wave-c4-d3b1-grok4-process-event-projection-brief.md
docs/execution/2026-08-04-wave-c4-d3b2-codex2-live-retained-brief.md
docs/execution/2026-08-04-wave-c4-d3b2-grok2-process-activity-brief.md
docs/execution/2026-08-04-wave-c4-d3b2-grok4-app-refresh-brief.md
docs/execution/2026-08-04-wave-c4-d3b2-grok4-workbench-mount-brief.md
docs/execution/2026-08-04-wave-c4-d3b2-grok4-workbench-repair-brief.md
docs/execution/2026-08-05-wave-c4-d3b3-grok4-run-cargo-cleanup-race-brief.md
docs/execution/2026-08-05-wave-c4-d3b4-grok1-native-git-brief.md
docs/execution/2026-08-05-wave-c4-d3b4-grok2-git-bridge-brief.md
docs/execution/2026-08-05-wave-c4-d3b4-grok3-git-pane-brief.md
docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-brief.md
docs/execution/2026-08-05-wave-c4-d3b4-grok4-git-workbench-repair-brief.md
docs/execution/2026-08-06-wave-c4-d3b4-grok4-real-desktop-git-e2e-brief.md
docs/execution/2026-08-07-wave-c4-conversation-list-race-report.md
docs/execution/2026-08-07-wave-c4-d3b3-cancel-tree-diagnosis.md
docs/execution/2026-08-07-wave-c4-d3b3-cancel-tree-rootcause.md
docs/execution/2026-08-08-ambient-glass-report.md
docs/execution/2026-08-08-dual-theme-report.md
docs/execution/2026-08-08-flake-analysis-VERDICT.md
docs/execution/2026-08-08-flake-analysis-report.md
docs/execution/2026-08-08-harness-flake-fixes-report.md
docs/execution/2026-08-08-knowledge-indexer-k0b-report.md
docs/execution/2026-08-08-settings-s0b-report.md
docs/execution/2026-08-08-step1a-contract-move-report.md
docs/execution/2026-08-08-ui-premium-research.md
docs/execution/2026-08-08-upstream-grokbuild-audit.md
docs/execution/2026-08-09-decouple-step1-report.md
docs/execution/2026-08-09-decouple-step3-report.md
docs/execution/2026-08-09-delivery-hardening-report.md
docs/execution/2026-08-09-firstrun-onboarding-report.md
docs/execution/2026-08-09-k0c-render-report.md
docs/execution/2026-08-09-k0d-ledger-report.md
docs/execution/2026-08-09-k0e-ledger-cli-report.md
docs/execution/2026-08-09-k1a-retirement-report.md
docs/execution/2026-08-09-k1b-handoff-render-report.md
docs/execution/2026-08-09-path-validation-robustness-report.md
docs/handoff/2026-08-01-pillow-controller-handoff.md
docs/handoff/2026-08-07-pillow-controller-handoff.md
docs/handoff/2026-08-08-pillow-controller-handoff.md
docs/migration/legacy-capability-inventory.md
docs/research/2026-07-30-grok-acp-protocol.md
docs/research/2026-07-30-grok-adapter-boundary.md
docs/research/2026-07-30-grok-security-acceptance.md
docs/research/2026-07-30-zbbody-reference-analysis.md
docs/superpowers/plans/2026-07-29-slice-a-desktop-foundation.md
docs/superpowers/plans/2026-08-01-c3-final-acceptance-checkpoint.md
docs/superpowers/plans/2026-08-05-c4-d3b3-provider-process-roundtrip.md
docs/superpowers/plans/2026-08-05-c4-d3b3-real-desktop-process-acceptance.md
docs/superpowers/specs/2026-08-08-settings-credentials-design.md
docs/verification/2026-07-29-slice-a-acceptance.md
docs/verification/2026-07-30-slice-b1-acceptance.md
docs/verification/2026-07-30-wave-b3-foundation-acceptance.md
docs/verification/2026-08-07-wave-c4-checkpoint.md
```

（另：`apps/desktop/tests/*` 若干行见 C2；`VaultSystem` 还可在更多 brief 中以 `<workspace>\VaultSystem\...` 形式出现，无盘符但仍绑定私有 sibling 布局。）

**vault wiki 硬引用（机外知识库）**

| 路径 | 行号 | 建议处置 |
| --- | --- | --- |
| `docs/handoff/2026-08-08-pillow-controller-handoff.md` | 439, 516 | **移私有**（指向 `vault\wiki\system\grok-usage.md` / `codex-usage.md`） |

---

### 2.1.D 本机环境假设：槽位 / 账号档 / 工具路径

| 路径 | 行号 | 类别 | 问题 | 建议处置 |
| --- | --- | --- | --- | --- |
| `AGENTS.md` | 47–60 | 槽位隔离运维 | 见 2.1.A | 移本地 |
| `docs/handoff/2026-08-08-pillow-controller-handoff.md` | 435–535（§8 整节） | **纯内部运维** | grok1–4 池、concurrency、占位 claim、容器 `~/.grokN`、另一项目 wrapper cwd 名、`codex1=Sol` / `codex2=Terra`、额度 4%、模型 id | **移到私有仓库**（不建议改写后仍放公开 handoff） |
| `docs/handoff/2026-08-07-pillow-controller-handoff.md` | 多处 Worker 路由 | 内部运维 | 派发与 Keeper 约束 | 移私有 |
| `docs/execution/*` 中约 **112** 个文件名含 `grok1–4` / `codex1–2` | 文件名级 | 内部运维叙事 | 槽位写进文件名与 brief 抬头 | **保留文件可**，但建议 `docs/README` 声明“历史 worker 标签非产品 API”；或批量改名（成本高） |
| `crates/knowledge-indexer/**` | 多处示例 `grok3`/`codex1` | 产品域 | 知识台账/派单视图把槽位标签当一等公民 | **保留但加说明**（这是产品能力）或开源版弱化为 generic `worker_label` 文档 |
| `docs/verification/worker-gate-entry.md` | 全文 | 半公开 | 绑定内部 Worker 门禁文化 | **保留**（对贡献者有价值）；去掉任何 Keeper 依赖（当前正文已以 `pnpm check` 为主，较好） |

**handoff §8 额外敏感点（不重复完整命令）**

| 路径 | 行号 | 类别 | 问题 | 建议处置 |
| --- | --- | --- | --- | --- |
| `docs/handoff/2026-08-08-pillow-controller-handoff.md` | 481 | 内部项目串扰 | 提到另一产品/仓库的 wrapper cwd 硬编码名 | 移私有 |
| 同上 | 518–519 | 账号运营 | 订阅档位昵称 + 剩余额度百分比 | 移私有 |
| 同上 | 511–512 | 本机布局 | 会话落点含 URL-encoded 仓库绝对路径模式 | 移私有 |

---

### 2.1.E 密钥形 / 邮箱 / 作者身份（核对）

| 路径 | 行号 | 类别 | 判定 | 建议处置 |
| --- | --- | --- | --- | --- |
| `scripts/check-secrets.mjs` | ~26–50, sample 块 | 扫描自检 | 假 `xai-`/`sk-`/`ghp_`/`BEGIN RSA PRIVATE KEY`；**历史** sample 曾用真实用户目录（已 scrub） | **保留**；确认 sample 永不回退真实用户名 |
| 产品 redaction 测试 | 多处 `xai-` / `auth.json` 断言 | 安全测试 | 假数据 | 保留 |
| `docs/verification/2026-08-07-wave-c4-checkpoint.md` | 151–157 | 说明 | 解释 `xai-` 误撞 | 保留 |
| Git author | 全历史 | 身份 | `keeper.local@localhost` — 无个人邮箱 | 保留或按总控统一 bot 身份 |
| 远程 URL 用户段 | `origin` | 身份暴露面 | 公开后可见 GitHub 用户/组织路径 | 非密钥；用户知悉即可 |
| `docs/research/2026-07-30-zbbody-reference-analysis.md` | 9 | 第三方 | Gitee `hwblack_admin/zbbody` | 确认是否可公开引用 |

**工作树：`Users\<HOST_USER>` 字面量 = 0**（`git grep` 无命中）。  
**工作树：真实密钥形态 = 0**（与总控初查一致）。

---

### 2.1.F 其它判断项

| 路径 | 行号/范围 | 类别 | 问题 | 建议处置 |
| --- | --- | --- | --- | --- |
| 几乎全部 `crates/*/Cargo.toml` `name = "keeper-*"` | package 名 | 对外品牌混淆 | 与 legacy Keeper 同前缀 | **保留但加说明**（ADR-0002）；未来 rename wave |
| `.gitignore` | 33 `/.superpowers/` | 策略缺口 | ignore 了目录但仍跟踪 2 文件 | 总控 `git rm --cached` 那 2 文件 |
| `.gitignore` | 无 `CLAUDE.local.md` 等 | 策略缺口 | 尚无本地 agent 覆盖约定 | 见 §3 拆分方案 |
| 无 `CONTRIBUTING.md` / `SECURITY.md` / `CODE_OF_CONDUCT` | 缺失 | 开源成熟度 | 不是“不该公开”，是“公开时缺的” | 建议后续补（本单不写） |
| `docs/research/2026-08-08-handoff-rendered-sample.md` | 全文 | 内部样例 | 渲染 handoff 样例，含派单表语义 | 可保留作 knowledge-indexer 夹具文档，或标 fixture |
| `docs/research/2026-08-08-knowledge-audit-run.md` | 全文 | 内部运维跑批 | 注册表审计输出 | 保留证据或标 archived |

---

## 3. `docs/` 分类清单（非一刀切）

当前跟踪 **`docs/` ≈ 315 路径**（md 为主；另有 references json 等）。  
结构（约）：

| 目录 | 约数 | 默认角色 |
| --- | ---: | --- |
| `docs/execution/` | 240 | brief / report / 少数 verdict·诊断 |
| `docs/verification/` | 26 | 验收与 checkpoint |
| `docs/research/` | 18+ | 调研（含本审计） |
| `docs/superpowers/` | 18 | specs + plans |
| `docs/handoff/` | 4 | 总控交接 |
| `docs/decisions/` | 4 | ADR |
| `docs/roadmap/` | 2 | 路线 |
| `docs/migration/` | 1 | 遗产盘点 |
| `docs/references/` | 2 | 上游 pin |

### 3.1 对外部读者**有价值**（建议公开，路径 scrub 即可）

| 集合 | 代表 | 价值 | 公开前处理 |
| --- | --- | --- | --- |
| ADR | `docs/decisions/ADR-0001`…`0004` | 产品边界、settings、knowledge actors | 去掉绝对盘符；说明 legacy 可选 |
| 设计规格 | `docs/superpowers/specs/*`（含 first design、C3 write、C4 process/git、settings） | 架构可信度 | scrub 路径；settings 文档确认无真实 secret 样例（当前为设计） |
| 实现计划（选择性） | `docs/superpowers/plans/*` 中**规格向**段落 | 可复现工程纪律 | 删“Owner: grokN/codexN/Keeper boot”派单头 |
| 调研 | `docs/research/2026-07-30-grok-*.md`、`2026-08-01-c3-*.md`、`2026-08-02-c4-*.md`、`2026-08-08-desktop-delivery-research.md`、`2026-08-08-knowledge-layer-research.md`、upstream audit | 协议/安全/交互深度 | scrub 绝对路径与 bridge 脚注 |
| 验收 | `docs/verification/*acceptance*`、`*checkpoint*` | 证明 gate 真跑过 | scrub 路径；弱化内部 wave 黑话或加术语表 |
| 迁移盘点 | `docs/migration/legacy-capability-inventory.md` | 清洁继承叙事 | 源路径改为逻辑名 |
| 上游基线 | `docs/references/*` | 可复现 pin | 保留 |
| 路线图 | `docs/roadmap/*` | 产品顺序 | 去掉 VaultSystem 强制语气 |
| Worker 门说明 | `docs/verification/worker-gate-entry.md` | 贡献者如何跑检查 | 保留 |

### 3.2 **纯内部运维**（对外人主要是困惑；建议移私有或归档出默认浏览路径）

| 集合 | 代表 | 为何不适合默认公开 | 建议处置 |
| --- | --- | --- | --- |
| 总控 handoff | `docs/handoff/2026-08-0{1,7,8}-pillow-controller-handoff.md` | claim/release、槽位池、额度、机外 wiki、bridge curl | **移私有仓库** 或 `docs/_internal/` + 强警告 + 默认不链 README |
| 派单 brief 洪水 | `docs/execution/*brief*.md`（约 **115**） | 一次性工人合同：白名单路径、槽位、Keeper boot、禁止项 | **保留作治理资产**但：① README 标明 historical；② 批量去掉 bridge 强制句；③ 或移 `docs/execution/archive/` |
| 槽位写进文件名的执行链 | 约 112 个 `*grokN*`/`*codexN*` execution 文件 | 暗示私有多账号农场 | 文件名可留历史；**正文**去账号运营细节 |
| Round 债务台账 | `docs/handoff/2026-08-08-round5-tech-debt-status.md` | 对内排期 | 可公开（低敏）或并入 GitHub issues |
| 渲染 handoff 样例 / audit run | `docs/research/2026-08-08-handoff-rendered-sample.md`、`...-knowledge-audit-run.md` | 运维输出快照 | 标 fixture；或仅测试树保留 |

### 3.3 **需要改写才能公开**（内容有价值，但绑定本机/私有假设）

| 集合 | 要改什么 |
| --- | --- |
| 几乎所有含 `<DRIVE>:\new\...` 的 report/brief/verification | 工作目录、`CARGO_TARGET_DIR`、示例命令 → 相对/`$REPO` |
| 所有指向 `VaultSystem\keeper|vault|references` 的句子 | → “optional external legacy tree / not in this repository” |
| research / report 顶部 “Keeper Bridge boot …” 段 | 删除或改为 “local orchestration optional” |
| plans 中的 Worker routing 表 | 改为角色（implementer / reviewer）而非 grok3/codex2 |
| `docs/execution/*report*`（约 114） | **保留证据价值**；统一 front-matter：`status: historical`；scrub 路径与 boot |

### 3.4 `docs/execution/` 再切一刀

| 子类 | 约数 | 公开策略 |
| --- | ---: | --- |
| `*brief*` | 115 | 低对外阅读价值；高历史合同价值 → 归档 + 说明，或私有 |
| `*report*` | 114 | 中高证据价值 → 公开 + scrub |
| `*verdict*` / 诊断 | 少 | 公开 + scrub |
| 误入 execution 的 research（如 ui-premium、upstream audit） | 少 | 可迁 `docs/research/`（总控后续） |

**总控立场对齐**：不建议整棵删除 `docs/`。建议 **分层**：

1. **默认贡献者路径**：ADR + specs + verification + research（scrub 后）+ 短 `docs/README.md` 地图；  
2. **historical execution/**：保留但声明非 onboarding；  
3. **handoff + 派单 runbook**：私有。

---

## 4. `CLAUDE.md` / `AGENTS.md` 拆分方案

### 4.1 目标

| 读者 | 应看到 |
| --- | --- |
| GitHub 贡献者 / 外部 agent | 如何构建、测试、架构边界、安全红线；**不**依赖本机 Keeper |
| 本机多 agent 农场 | Bridge boot、槽位 claim、vault wiki、record-only 细则 |

### 4.2 建议文件布局

```text
# 公开（跟踪）
AGENTS.md                 # 精简工程宪法
CLAUDE.md                 # 薄入口：仅 “读 AGENTS.md + CONTRIBUTING”
CONTRIBUTING.md           # （建议新建）构建、pnpm check、PR 期望
docs/README.md            # （建议新建）文档地图与 historical 声明

# 本机专用（不跟踪）
AGENTS.local.md           # 或 .pillow/AGENTS.local.md
CLAUDE.local.md           # 可选；工具若支持 include
.cursor/rules/local-vault.mdc
.pillow/ops/keeper-bridge.md   # boot/claim/release 手册（从 handoff §8 抽出）

# .gitignore 增补建议
AGENTS.local.md
CLAUDE.local.md
.cursor/rules/local-*.mdc
.pillow/ops/
# 保持已有：/.keeper/  /.superpowers/  .env* 等
```

### 4.3 公开 `AGENTS.md` 建议保留章节

1. Product order（IDE → knowledge → ops）  
2. Engineering rules（SQLite 权威、shell 不拥 domain、adapter、approval、project-scope、测试要求）  
3. 安全：禁止提交凭据 / 机器用户路径 / 实时 session  
4. 指向 `docs/verification/worker-gate-entry.md` 与 `pnpm check`  
5. **可选**一节 “External orchestration”：若维护者自有调度系统，可记录 session；**非必须**  

### 4.4 公开 `AGENTS.md` 应删除或外移

- Keeper is record-only 长段（可缩成一句“本仓库不绑定外部 session 服务”）  
- 整节 **Grok CLI Slot Isolation**（`grok1–4`、`GROK_HOME`、`auth.json` 污染）  
- 任何 `VaultSystem\...` 路径  

### 4.5 公开 `CLAUDE.md` 建议正文（概念稿）

```markdown
# Pillow Agent Entry

Read and follow `AGENTS.md` in this repository.
For build and PR expectations see `CONTRIBUTING.md` (when present).

Do not assume a local session bridge, vault wiki, or sibling
`VaultSystem` checkout exists. All product work is self-contained
in this repository unless a maintainer task says otherwise.
```

### 4.6 本地 `AGENTS.local.md` / ops 应承载

- `POST <BRIDGE>/api/agent-bridge/boot` 契约与 `parentSessionId`  
- claim/release、capability header  
- 槽位 ↔ 容器映射、禁止只设 `GROK_HOME`  
- vault wiki 路径（仅本机）  
- 与 `docs/handoff` 私有副本的交叉链接  

### 4.7 Cursor 规则

| 动作 | 说明 |
| --- | --- |
| 取消跟踪 `_vault-shared.mdc` | 或改成不 `alwaysApply`、且无绝对路径的可选规则 |
| 本地 vault 规则 | 仅存在于开发者机器，gitignore |

### 4.8 工具 include 现实

| 工具 | 策略 |
| --- | --- |
| Claude Code | 公开薄 `CLAUDE.md`；本地用 `CLAUDE.local.md`（若版本支持）或用户级全局规则 |
| Cursor | 公开无 vault；本地 `local-*.mdc` |
| 通用 | **以公开 `AGENTS.md` 为唯一跟踪权威**；本地文件覆盖运维 |

---

## 5. 历史处置的事实依据（**不含结论**）

### 5.1 本机用户路径 `Users\<HOST_USER>` 出现的提交

`git log --all -S "Users\<HOST_USER>"`（及等价反斜杠形式）命中 **恰好 3 个 commit**：

| Commit | 主题（短） | 工作树/ blob 行为 | 字面量落点（文件） | 约计数 |
| --- | --- | --- | --- | --- |
| `a2fda9c` | secret/machine-path gate | **引入** scanner sample | `scripts/check-secrets.mjs` 的 `sample:` 使用 `C:\Users\<HOST_USER>\.cargo` | **+1** 处 sample |
| `30adf16` | ambient glass UI 等 | **引入** handoff 字面启动命令 | `docs/handoff/2026-08-08-pillow-controller-handoff.md` 内多行 ` /c/Users/<HOST_USER>/...` 与 `USERPROFILE='C:\Users\<HOST_USER>\.grokN'` 等 | **+5** 行量级（同文件） |
| `7679f6e` | storage barrier 等 | **删除** handoff 字面路径；**替换** scanner sample | 对 handoff 与 `scripts/check-secrets.mjs` 的 `-` 行 | 从 tip 工作树消除；**历史 blob 仍在** |

**当前 `HEAD` 工作树**：无 `Users\<HOST_USER>`。  
**克隆完整 git 对象后**：仍可用 `git show a2fda9c:scripts/check-secrets.mjs`、`git show 30adf16:docs/handoff/...` 读出历史字面量。

### 5.2 `.superpowers/` 历史跟踪事实

| 路径 | 性质 | 生命周期（事实） |
| --- | --- | --- |
| `.superpowers/sdd/task-4-report.md` | 早期 agent 任务报告 | 引入后由 `dfdac17` **删除**（chore: keep agent reports out of source control） |
| `.superpowers/sdd/grok-shell-prototype-report.md` | 原型实现报告 | **仍跟踪** |
| `.superpowers/sdd/wave-b1-ui-report.md` | B1 UI worker 报告（含 worktree 绝对路径） | **仍跟踪** |
| `.superpowers/sdd/wave-b2-protocol-research.md` | 研究 brief | 曾跟踪；后续不在 `git ls-files`（内容进 `docs/research` 系） |
| `.superpowers/sdd/wave-b2-boundary-research.md` | 研究 brief | 同上 |
| `.superpowers/sdd/wave-b2-security-research.md` | 研究 brief | 同上 |

`.gitignore` 已有 `/.superpowers/`，但 **gitignore 不使已跟踪文件退场**；历史仍含上述 blob。

内容性质概括：实现/研究 **过程文档**，非凭据；含分支名、gate 结果、局部绝对 worktree 路径。

### 5.3 若选择**压缩历史 / orphan 新根**会失去什么（事实清单，非建议）

除“提交图与时间线”本身外，公开克隆者还将失去或难以审计：

| 资产 | 说明 |
| --- | --- |
| 134 个 commit 的因果链 | 含 wave 合并、fixup、gate 修复叙事 |
| 已删文件的可恢复性 | 例如曾删的 `.superpowers/sdd/task-4-report.md`、中间 handoff 字面路径版本 |
| `git blame` 跨开源前后的连续性 | 贡献者归因重置为新根作者 |
| 已发布 SHA 的可引用性 | 外部若已引用 `30adf16` 等 SHA 会断裂 |
| 分支 tip 与 main 的关系证明 | 本地仍有多条 `wave-b2-*`、`feat/*` 分支名（是否 push 到 GitHub 需总控另查）；压缩 main 不等于清理 remote 其它 refs |
| 签名/tag（若有） | 本审计未枚举 annotated tags |

**不会自动失去**（若新树仍包含当前 blob）：当前工作树源码、`docs/` 现文件、LICENSE、测试与脚本。

### 5.4 若选择**保留完整历史**，公开后的暴露面（事实）

| 暴露面 | 具体 |
| --- | --- |
| 本机用户目录名 | 3 个 commit 的 blob 中可读 `<HOST_USER>` |
| 本机开发布局 | 大量历史与**当前** docs 中的 `<DRIVE>:\new\...`、sibling `VaultSystem`、worktree 路径 |
| 本地运维 API 面 | Bridge 端口与路径、`claim`/`release`/`capability` 形状（无密钥，但是私有协议说明书） |
| 多账号运营细节 | handoff 历史/当前中的槽位池、额度叙事、模型档位昵称 |
| 过程文档 | 全部 brief/report 合同与失败 boot 旁白 |
| 作者身份 | 仅 `keeper.local@localhost`（低敏） |
| 扫描器假密钥 | `a2fda9c` 起存在；**不是**真密钥，但会触发外部 secret scanner 噪音 |
| **未发现** | 历史 pickaxe 范围内的真 `ghp_` 生产 token、真私钥文件（与总控初查一致；**未**全量 134 commit 字节级再扫） |

### 5.5 与“仅 scrub 工作树”的关系（事实）

- 工作树清理 **不** 移除 `a2fda9c` / `30adf16` 中的用户路径 blob。  
- 历史改写（filter-repo 等）可移除这些 blob，但改变所有下游 SHA——**本单不执行、不推荐与否**。

---

## 6. 不确定项

| 项 | 为何不确定 |
| --- | --- |
| 其它 refs 是否已 push 到 GitHub | 仅确认 local `origin` URL 与 `main` 跟踪；未枚举 `git ls-remote` 全部分支内容（避免网络写/依赖） |
| `artifacts/*.png` 是否含 UI 中偶然路径文字 | 未 OCR |
| Gitee `zbbody` 是否允许在 README 级公开引用 | 需 license/上游意图确认 |
| knowledge-indexer 是否计划成为对外产品能力 | 若是，槽位语义是功能；若否，文档应降级 |
| handoff 中额度数字是否曾在更多 commit 出现 | 当前 tip 有；未全历史搜“额度” |
| `Cargo.lock` 本地修改是否含敏感 | 通常无；未展开 diff（只读任务下观察到 `M Cargo.lock`） |
| 全 134 commit 是否存在其它用户名/邮箱 | author 固定；**committer/Co-Authored-By** 见 `30adf16` 含 `Claude Opus … <noreply@anthropic.com>`（工具署名，非个人） |

---

## 7. 建议处置优先级（供总控排期；非历史结论）

| 优先级 | 动作 | 对象 |
| ---: | --- | --- |
| P0 | 拆分/改写 agent 入口 | `CLAUDE.md`、`AGENTS.md`、`.cursor/rules/_vault-shared.mdc` |
| P0 | handoff 运维手册出公开树 | `docs/handoff/2026-08-08-…` 等 |
| P1 | 停止跟踪误跟踪的 superpowers 报告 | `.superpowers/sdd/*`（2 文件） |
| P1 | scrub 绝对路径 | §2.1.C 文件清单 + 测试夹具 |
| P2 | docs 分层与 `docs/README` 地图 | execution historical vs ADR/specs |
| P2 | README 对外化 | 缩短 wave 流水账 |
| P3 | crate `keeper-*` 命名 | 独立 rename wave |
| 史 | 用户路径 blob | 仅提供 §5 事实；由总控/用户选择 filter 或保留 |

---

## 8. 审计元数据

| 项 | 值 |
| --- | --- |
| Tracked files | 782 |
| Commits on HEAD | 134 |
| Docs paths | ~315 |
| `<workspace>`+`<workspace>` 命中行 | ~121 |
| `VaultSystem` 命中文件（docs） | ~61 |
| Bridge `127.0.0.1:7890` 命中文件 | 8 文本文件（外加 `Cargo.lock` 校验和伪命中 1） |
| 工作树 `Users\<HOST_USER>` | 0 |
| 历史含 `Users\<HOST_USER>` 的 commit | 3（`a2fda9c`, `30adf16`, `7679f6e`） |
| Keeper Bridge boot（本 worker） | HTTP 400；审计不依赖 bridge 记录 |

---

**PRE-OSS-AUDIT-COMPLETE**
