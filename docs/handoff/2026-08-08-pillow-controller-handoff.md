# Pillow 总控交接手册(2026-08-08)

工作目录:`<repo>`
分支 / HEAD:`main` / `8f563db6cedb34eb336c4e76229b24eb7f3c47bd`(本地 = 远程)
远程:`https://github.com/dfan0519-tech/pillow.git`(**private**,账号 `dfan0519-tech`)

> 本文件取代 `docs/handoff/2026-08-07-pillow-controller-handoff.md` 与
> `docs/handoff/2026-08-08-round5-tech-debt-status.md`。旧文件描述的是 C4 验收期的状态,
> 现已过时,不得作为真相源。

---

## 1. 一句话状态

C4 已 checkpoint 并推送。当前正在做**框架契约冻结前的准备**与**品牌/视觉升级**。
Round 8/9 全部 Worker **已收工并验收通过**,成果已提交推送。**在途 Worker:0。**
下一步是在干净树上派 Codex 做 storage barrier 实验(§6.5)。

---

## 2. 产品终点(用户已明确,这是后续所有设计的依据)

用户 2026-08-08 亲口确认的三点:

1. **做工业级产品**,不是自用工具。
2. **Obsidian 是"做大项目的模式"**:harness 按固定路径**自动填充/淘汰知识库**,
   解决上下文与项目混乱问题,提升沟通效率。
   —— 注意:这个项目**自己就是第一个用户**(`docs/` 已 261+ 文件,交接靠人手写"本文件取代 X")。
3. **做框架,让更多人用**。"一人公司"是可复制的方法论,不是自己经营。

三阶段路线不变:Stage 1 工业级本地 AI IDE → Stage 2 治理型 Obsidian 知识层 →
Stage 3 一人公司项目操作系统。**当前仍在 Stage 1。**

### 两条待拍板的红线(Stage 2 开工前必须定)

1. **SQLite 保持域权威,Obsidian 是投影 + 人类可编辑入口,写回走审批路径。**
2. **"自动淘汰"只做标记衰减 + 归档;真删除必须走审批并留收据。**

用户尚未逐条确认这两条,Stage 2 设计前需再问一次。

---

## 3. 四步计划与进度

| 步骤 | 内容 | 状态 |
| --- | --- | --- |
| 1 | 契约整理 + 门加固 | **基本完成**(见 §4) |
| 2 | 设置层 + 凭据管理 | **设计已交付**,未实施 |
| 3 | 知识层原型(Obsidian 模式) | 未开始,需红线拍板 |
| 4 | D 多 Runtime 扩展 | 未开始,放最后 |

**重要修正**:原以为"凭据管理"是对外交付硬阻塞。设计提案
(`docs/superpowers/specs/2026-08-08-settings-credentials-design.md`)证明**不是**——
最小可交付是 **S0:SQLite 设置 + runtime/可执行路径/profile 的 UI + composition 读取**;
凭据(S1)可推后,代价是无法产品化 API-key 类 provider。
该文档 §13 有 Q1–Q9 待总控拍板,每项都有推荐。

---

## 4. 已完成的关键工作(近几轮)

### 4.1 C4 checkpoint(`docs/verification/2026-08-07-wave-c4-checkpoint.md`)

Round 4 验收抓出四个此前所有门都漏掉的缺陷,其中一个 Critical
(Git Changes 把已验收的 C2 资源浏览器挤出 UI)。

### 4.2 可移植路径规则收敛(`8f563db`)

**同一条不变量曾被独立实现四遍**,导致连续两轮"以为修好了":
解析器 → 项目路径映射 → 渲染层 DTO → 命令层。
现已收敛为单一 Rust 事实源 + `tests/fixtures/portable-path-cases.json` 跨层契约表
(Rust 与 TS 各跑同一张表)。**这是本项目最重要的架构教训。**

### 4.3 门加固

- `scripts/run-worker-gate.mjs` = **唯一合规门入口**(`pnpm gate` / `pnpm check`)。
  文档:`docs/verification/worker-gate-entry.md`
- 修掉 feature 盲区:`cargo test -p keeper-runtime-host` 曾只编 16 个测试,
  workspace 编 19 个(desktop dev-dependency 启用 `test-support` + Cargo feature 统一)。
  现由 runtime-host 自 dev-dependency 补齐。
- `scripts/check-secrets.mjs`:**自检式**密钥/机器路径扫描器,已接入门。
  **它先用已知阳性样本验证每条模式,失效则 fail-closed** —— 这个设计来自一次真实翻车
  (手写 grep 转义错误导致假阴性,差点把机器路径推上远程)。
- `scripts/check-acp-protocol-drift.mjs`:ACP 协议漂移检测。
  adapter **手写实现**线协议,不依赖 `agent-client-protocol`,故无编译期信号。

### 4.4 上游 grok-build 审计

`docs/execution/2026-08-08-upstream-grokbuild-audit.md`:
上游自基线 `5da6962e` 起 9 次 sync、版本串到 `1.0.0`,但
`agent-client-protocol` 0.10.4 / schema 0.11.4 的 **Cargo.lock 校验和逐字节一致**,
`xai-acp-lib` diff 为空。**我们的耦合面零破坏,不需要改 adapter。**
可选动作:把文档钉点推到 `afbc0fb`(纯文档)。

### 4.5 四个 flake 的 Codex 深度分析(重要)

`docs/execution/2026-08-08-flake-analysis-VERDICT.md` 与 `-report.md`
(Codex Sol / max effort,只读)。

**推翻了"共享全局污染"假说**(每测独立 tempdir、独立 app-data/webview/grok profile),
确立真正共性:**测试与 harness 用时间窗口代替确定性阶段握手**,并行只是放大器。
并明确:**横切策略(如串行化)不能替代四处独立补丁。**

| flake | 判定 | 状态 |
| --- | --- | --- |
| `cancel-tree` | **高置信,harness bug**:`spawnSync` 无 `timeout`,阻塞越过产品 60s 时限,`composer-cancel` 合法卸载 | **已修**(未提交) |
| `slow_output` | 高 ~0.88,测试缺阶段握手 | **已修**(未提交) |
| `storage_failure_after_live` | **0.95,产品邻近竞态**:主线程 `reconcile_expired_for_owner` 与 waiter `finalize_expire` 都在离锁后调同一 transition helper → 同一次过期被转换两次 | **已坐实并修复**(见 §4.6) |
| `two-project-isolation` | 机制中置信 / 归因低置信,**证据不足** | 保持观察,等 RED 现场 |

### 4.6 storage 双 transition:barrier 实验坐实并修复

报告:`docs/execution/2026-08-08-storage-barrier-verdict.md`(Codex Sol / max)。

**这是"用确定性实验取代概率观察"的范例,值得复用。**
此前我跑过 30 次串行循环全绿,零信息量 —— 因为**连续全绿不能推翻内部竞态**。
决定性做法是加一个**仅测试可见的 proposal-scoped barrier**,强制两条路径都越过
expiry 判断,再看有几笔 transition。

- **修复前**:`main-list` 与 `waiter-expiry` 都过 barrier,两笔 transition,`delta=2`,
  测试按既有语义失败(`left: 2, right: 1`)。
- **反向控制**:用 channel 暂停 waiter → `delta=1`,排除了主线程链内重试与第三写入口。
- **修复**:runtime-host live coordinator 里给 terminal transition 墓碑加
  **proposal-scoped single-flight**;winner 离锁后执行唯一一次 SQLite transition,
  loser 在既有 condvar 上等待且**只接受真实 durable terminal**,否则返回 opaque `Gateway`。
  SQLite schema、transition 校验、持久化顺序、对外 receipt/error 映射**均未改变**。
- **修复后(总控独立复跑)**:两条路径仍都过 barrier,但只剩一笔 transition,`delta=1`,
  两个调用方都拿到 `Gateway`;durable row 仍为 `PendingApproval`,不伪造 terminal。

**生产不可见性(总控独立核验,未采信自报)**:插桩的字段、类型、方法、
甚至 `transition_source: &str` **函数参数本身**都被 `#[cfg(any(test, feature = "test-support"))]`
门控;1005 行附近可见成对的 `#[cfg(not(...))]` 生产分支直接调用原 `store.transition(...)`。
另按 crate 单独 `cargo check`(不启用 `test-support`)通过。

⚠️ 总控修了它遗留的一处 clippy `collapsible_if`(`governed_write.rs:1005`)。
Codex 的门当时被 §9.8 那条 secrets 挡在编译之前,所以它没机会看到这个 lint。

---

## 5. Round 8/9 成果与验收结论

### 5.1 UI 环境光晕 + 磨砂玻璃(grok3,**已验收**)

简报:`.superpowers/round9/BRIEF_UI_ambient_glass.md`
报告:`docs/execution/2026-08-08-ambient-glass-report.md`

核心洞察(必须理解):之前加 `backdrop-filter` 在深色下读不出来,不是玻璃错了,
是**背后是纯色平面、没东西可透**(调研实测 `app→main→elevated` 对比仅 1.05–1.07:1)。
用户给的参考图解法是**在玻璃背后铺大面积虚化色场**。
所以这单地基是**环境光晕**,磨砂只是结果。

⚠️ 简报里显式**不采纳** UI 调研报告中"深色下关闭无效 blur"那一条,并说明了理由;
其它三条(hover/active 去淡紫、加强边框、拉开表面阶梯)仍采纳。

**验收结论(总控独立复算,未采信 Worker 自报数)** —— 表面阶梯:

| | app→main | main→elevated |
| --- | --- | --- |
| 浅色 | 1.130:1 | 1.189:1 |
| 深色 | 1.163:1 | 1.201:1 |

浅色两值与 Worker 报告逐位吻合;深色它未报,实际优于浅色。对照改前 1.05–1.07:1,
截图中侧栏 / 主舞台 / 资源栏三层已肉眼可分。正文对比:深色 15.8 / 9.9 / 5.8:1,
浅色 8.9 / 5.6:1。内容区红线守住(终端输出仍是小圆角、高对比、无模糊实心块),
模糊只在容器级 chrome。testid / 可访问名 / 布局几何零改动,视觉契约测试只加强。

**两处残留(已接受,非阻塞)**:

1. 浅色 `--text-tertiary` 实测 **3.57:1** —— 过大字号 3:1,不过正文 4.5:1。
   Worker 报"两皮肤正文均 ≥4.5:1";严格说 tertiary 不算正文,但需记档。
2. **浅色皮肤无任何截图证据** —— E2E 只截深色,视觉验收目前是单皮肤的。
   补浅色截图应与 S0 设置切片(主题切换持久化)一并做。

### 5.2 harness flake 修复(已验收)

`scripts/wave-c4-*-acceptance.mjs`、`crates/runtime-host/tests/governed_process_bridge.rs`
报告:`docs/execution/2026-08-08-harness-flake-fixes-report.md`

### 5.3 总控补的门缺口(本轮新增)

`run-worker-gate.mjs` 原有 clippy、密钥闸、ACP 漂移检测,**唯独没有 `cargo fmt --check`**。
后果:Worker 能带着未格式化的 Rust 拿到**绿门**,只有总控独占关卡才拦得住
(本轮就拦到一处 `governed_process_bridge.rs` 的 `panic!` 折行)。
已把 `cargo-fmt` 作为**编译前的廉价步骤**插在 secrets 之后、typecheck 之前。

⚠️ 门只在**失败时**打印步骤名,成功路径无任何步骤级输出。诊断时只能靠
"走到了后面的步骤" 反推前面的步骤通过了。若要提升可观测性,给每步加一行
`console.log` 即可,但**改门后必须重跑一次完整门**才算数。

### 5.3 配色决定(用户已定)

**保留品牌淡紫,只学参考图的手法。** 品牌色板:
`LILAC HAZE #DCCBFF` / `OFF-WHITE #FAF9F7` / `SMOKE GRAY #C7C7CE` /
`GRAPHITE #3E3E46` / `BLACK INK #0E0E10`。

**桌面密度红线**:内容区(代码/diff/终端/文件列表)**不模糊、不大圆角、高对比**。
参考图是移动端冥想 app,每屏三五个元素;我们是 IDE。

---

## 6. 下一步

1. ~~等 UI Worker 收工 → 释放租约~~ **已完成**(grok 占用归零)
2. ~~独占跑门 + E2E~~ **已完成**:门 exit 0(前端 380/380)、E2E 10/10、
   `git diff --check` 0;`cargo fmt --check` 曾 exit 1,已修并把该步骤补进门(§5.3)
3. ~~人工看截图验收视觉~~ **已完成**,结论与残留见 §5.1
4. ~~提交推送~~ **已完成**
5. ~~在干净树上派 Codex 做 storage barrier 实验~~ **已完成并验收**,见 §4.6
6. **下一步:S0a 设置层存储 + 解析服务**(Rust,不含 UI)
   简报已备好:`.superpowers/round10/BRIEF_S0A_settings_storage.md`
   口径已由 **ADR-0003** 拍死(Q1–Q9 全部有结论,Q4/Q6 明确推到 S1)。
   **S0 按串行两片走,不并行** —— 坑 #1(白名单重叠)与坑 #2(抢 `target/`)
   都是并行切出来的。S0b(UI + `runtime_composition` 接线)等 S0a 收工再派。
7. 再之后:双皮肤持久化(顺带补浅色截图证据)→ 知识层设计(需先拍 §2 两条红线)

⚠️ 用户明确要求:**两个 codex 槽位保证能正常调用,不得违规操作让账号有风险**。
派 codex 前必须先确认全机没有在跑的 `codex exec`(按命令行查,**不要按进程名杀**)。

---

## 7. 未解决风险

1. **`two-project-isolation` 归因未知**。取证已就位,下次 RED 是第一次真正机会。
   **无 RED 前不要改产品 cancel/approval 路径。**
2. ~~`storage_failure_after_live` 竞态未验证~~ **已坐实并修复**(§4.6)。
   残留:跨进程同时写同一 SQLite 不在本次进程内 coordinator 实验范围内。
3. **adapter 仍依赖 `runtime-host`**。契约类型已搬到 `agent-protocol`,
   但 adapter 还要调 `execute_governed_*`。彻底解耦需把治理操作做成注入的 trait。
   **D 阶段接第二个 runtime 前应做掉。**
4. **UI 观感尚未达标**。表面阶梯是主因,本轮正在改;标识在 22px 下双面明度差损失严重
   (有物理上限),最终 3D 标识应由设计源导出替换。
5. **门与并发派单冲突**:唯一门入口 = 每个 Worker 跑完整 workspace cargo,
   N 个并行 Worker = N 份并发构建抢同一 `target/`,已造成 LNK1104 与多次误报失败。
   **尚未系统化解决**(可选:门运行串行化,或每 Worker 独立 `CARGO_TARGET_DIR`)。

---

## 8. 操作手册(Worker 派发)

### 8.1 Grok(4 槽位,4 个独立账号)

**槽位固定、账号流动**,只引用槽位名不写死邮箱。权威文档:
`<workspace>/VaultSystem\vault\wiki\system\grok-usage.md`

```bash
# 取 capability(claim/release 需要)
CAP=$(curl -s -X POST "http://127.0.0.1:7890/api/agent-runner/capability" \
  -H "Content-Type: application/json" --data '{}' \
  | node -e "let s='';process.stdin.on('data',d=>s+=d).on('end',()=>process.stdout.write(JSON.parse(s).capability||''))")

# claim(注意:池子会先填满一个槽的 2 个实例才换槽;
# 要拿到 4 个不同槽位,需用 filler claim 占位再释放)
curl -s -X POST "http://127.0.0.1:7890/api/agent-runtimes/grok-cli/claim" \
  -H "Content-Type: application/json" -H "X-Keeper-Runner-Capability: $CAP" \
  --data '{"project":"pillow","task":"<task>","agentName":"<name>"}'

# release
curl -s -X POST "http://127.0.0.1:7890/api/agent-runtimes/grok-cli/release" \
  -H "Content-Type: application/json" -H "X-Keeper-Runner-Capability: $CAP" \
  --data '{"leaseId":"<lease>"}'
```

**启动**(wrapper 硬编码了 denglou-ui 的 cwd,且 `--cwd` 不可重复,所以直接调):

`<USER>` = 本机用户名,**不写进仓库**(密钥闸会拦);实际值见上面那份 vault 文档。

```bash
# grok1(默认容器)
/c/Users/<USER>/.grok/bin/grok.exe --cwd <repo> -m grok-4.5 \
  --always-approve --effort high --prompt-file '<绝对路径简报>' > <log> 2>&1

# grok2/3/4(复制 wrapper 的三重容器重定向,只改 cwd)
USERPROFILE='C:\Users\<USER>\.grokN' \
APPDATA='C:\Users\<USER>\.grokN\AppData\Roaming' \
LOCALAPPDATA='C:\Users\<USER>\.grokN\AppData\Local' \
/c/Users/<USER>/.grok/bin/grok.exe --cwd <repo> ...
```

⚠️ **绝不只设 `GROK_HOME`** —— 血训:会导致两进程跑同一主号、双倍消耗打到 402。
⚠️ `--effort` 最高是 `high`,**没有 xhigh**。

**路由验证**(每次派发后做):检查会话落点
`<容器>/.grok/sessions/<repo>/<session-id>/`

### 8.2 Codex(2 槽位)

权威文档:`<workspace>/VaultSystem\vault\wiki\system\codex-usage.md`

- `codex1` = Sol(`gpt-5.6-sol`,**pro**,能力最强,用于攻坚/代码审核)
- `codex2` = Terra(`gpt-5.6-terra`,plus,**额度仅剩 4%**,基本不可用)

```bash
export PATH="$HOME/AppData/Roaming/npm:$PATH"
codex exec --cd <repo> --dangerously-bypass-approvals-and-sandbox \
  --skip-git-repo-check -m gpt-5.6-sol -c model_reasoning_effort="max" \
  -o <report> - < <brief> > <runlog> 2>&1
```

**铁律**:
- **全机同时只跑 1 条 `codex exec`**;并发靠简报里写死的内部 fan-out 子代理。
  一条 exec = 一个 session 额度,内部并发不成倍烧额度。
- **绝不按进程名杀 codex**(会误伤别的会话);只按自己的 log/产出文件追踪。
- **不采信 stdout 终报**,以产出文件实质内容 + `git diff` 为准
  (实测 Codex 会在子代理已成功落盘时主控终报写"被阻断")。
- Keeper **没有** codex 的 claim/release 接口,只做只读健康与额度上报。
- 撞额度/402 → **停手报用户**,不擅自换槽。
- `--dangerously-bypass-approvals-and-sandbox` 绕开 Windows 共享沙箱账号,
  **避免** 1326/1909 锁定;代价是子进程以当前用户直跑,
  **只可用于本机可信仓库,绝不用于审查不可信代码**。

### 8.3 简报必备要素(经验证有效)

一句话定位 + 必读文件 + **已确认事实(禁止重新论证)** + 总控已定死的设计口径 +
**精确写入白名单** + 硬禁止 + 必须跑的门 + 报告路径 + 完成/阻塞信号。

**必写的硬禁止**:不得 git 写操作、**不得跑桌面 E2E(总控独占)**、
不得动 `.superpowers/` 既有证据、不得读打印 auth/token、
不得为让断言通过而降低 authority/privacy/cleanup 标准。

---

## 9. 我踩过的坑(务必避免重复)

1. **白名单重叠**:曾把 `crates/*/Cargo.toml` 同时给两个 Worker,导致"自依赖被并发覆盖"。
   按目录切簇时要检查 manifest 这类**横切文件**。
2. **文件簇不相交 ≠ 构建产物不相交**:多个 Rust 任务共用 `target/` → LNK1104 链接器锁。
   一个纯 CSS 任务因此拿不到绿门。
3. **`git add -A` 吸进构建产物**:一个 Worker 自建 `target-step1c-gate/` 规避争用,
   未被 `.gitignore` 覆盖,一次 `add -A` staged **12,599 个文件**。
   已加 `/target-*/` 规则。**提交前务必看 staged 文件数。**
4. **扫描器假阴性**:手写 grep 转义错误导致"无发现",而路径就在第 878 行。
   **清扫类检查必须先用已知阳性样本验证扫描器本身。** 这是 `check-secrets.mjs` 自检设计的由来。
5. **跑了对照组当实验**:Codex 的实验 C 要求先加 barrier;我跑了它标注为
   "非决定性概率对照"的循环,30 次全绿,零信息量。**读实验设计要读全。**
6. **简报耦合导致无谓 BLOCKER**:把"两套皮肤"(纯 CSS)和"切换持久化"(需存储)
   绑在同一条 BLOCKER 上,导致本可交付的皮肤工作也黄了。**能解耦就解耦。**
7. **`shell: true` + 含空格的绝对路径**:`run-worker-gate.mjs` 曾把
   `C:\Program Files\nodejs\node.exe` 在空格处截断,**把通过的步骤误报成 FAILED**。
   shell 只给 `.cmd` shim 用。

8. **绿门源于"扫描器看不见",而不是源于干净**(本轮亲手踩,已修)。
   `check-secrets.mjs` 原来只扫 `git ls-files`(**已跟踪**文件)。交接手册带着
   `C:\Users\<真实用户名>\.grokN` 一路 untracked,**两次跑门都是绿的**;
   我一提交,门立刻在 main 上变红,而机器路径已经推到远程了。
   —— 扫描范围现改为 `--cached --others --exclude-standard`,即
   **"将会被提交的集合"**,`.gitignore` 仍然生效。

   同时补了第二个盲点:`user-profile-path` 只认盘符写法 `C:\Users\<name>`,
   **不认 MSYS 写法 `/c/Users/<name>`** —— 我们全程在 Git Bash 下工作,
   后者才是常见的那种。
   扩面后**立刻**在 Codex 报告里抓到一条二次泄漏(它把该值原样引用进了报告)。

   > 这是坑 #4 的同一族:**先问"扫描器能不能看见它",再看它报没报**。
   > 每条模式现在都带多个已知阳性样本,任一分支失效即 fail-closed。

---

## 10. 环境注记

- **`pnpm` 不在 bash PATH**。用 `corepack pnpm`,或
  `export PATH="/c/Program Files/nodejs:$PATH"`(`pnpm gate` 会自己找 corepack)。
- **`gh` 在 `C:\Program Files\GitHub CLI\`**,不在 PATH。
- 本机 Git 在 `F:\Git\`(非标准路径);产品与 harness 的候选列表**已移除该硬编码**,
  靠 PATH 回退或 `PILLOW_HOST_GIT` / `PILLOW_TEST_GIT` 覆盖。
- 一个 Worker 曾自行 `corepack enable`,把 pnpm shim 装进 `C:\Program Files\nodejs\`
  (仓库之外的系统改动,已记录)。
- `.superpowers/` 已 gitignore,所有简报/日志/证据都在那里,不进提交。

---

## 11. 交接完成状态

- 在途 Worker:**0**;Codex 进程:**0**
- Round 8/9/10 成果已提交推送;暂存区空,HEAD = 远程
- 门 / E2E(10/10)/ fmt / diff 四项全绿;barrier 实验总控独立复跑 `delta=1`
- **下一单已备好**:S0a 设置层(`.superpowers/round10/BRIEF_S0A_settings_storage.md`),
  口径见 ADR-0003
- Stage 2 的两条红线**尚未获用户逐条确认**,设计前需再问
- 未决:`two-project-isolation` 归因(等 RED;本轮 E2E 它是绿的)、
  adapter 与 runtime-host 解耦、门与并发派单的 `target/` 争用、
  浅色皮肤截图证据、浅色 `--text-tertiary` 3.57:1
