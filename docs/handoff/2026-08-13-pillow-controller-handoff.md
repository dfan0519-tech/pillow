# Pillow 总控交接手册(2026-08-13)

工作目录:`<repo>`
分支 / HEAD:`main` / `992acd7`(**本地 = 远程,工作树干净**)
Release 二进制:`target/release/pillow.exe` @ **15:31**
🔴 **它落后于 HEAD** —— 不含最后两轮(两级权限、过宽规则)。见 §8。

> 取代 `docs/handoff/2026-08-12-*.md`。那份里的**槽位表是错的**,
> 已在原文件内更正,但以本份为准。
> 运维细节在 `.superpowers/RUNBOOK.md` 与 `.superpowers/CODEX-DISPATCH.md`(gitignored)。

---

## 1. 一句话状态

**外审 6 条全清;权限模型从"只在渲染层成立"变成真的;并发语义已拍板并落文档。**

🔴 **用户仍未跑过一轮真实任务。** 这条从 8-11 记到现在,**没有变**。
现在功能面基本齐了,这是唯一还完全没有实盘证据的环节。

---

## 2. 🔴 今天最该记住的一件事

**测试通过的理由,常常不是它自称在测的那个理由。** 今天撞了三次:

| # | 表现 | 怎么发现的 |
| --- | --- | --- |
| 1 | EA-04 宿主守卫只有**源码字符串测试**盯着 —— 把守卫改成死代码,整套照绿 | 我变异验证时发现 |
| 2 | EA-03 命令层拒空 pattern 的测试,**拆掉哪一层都绿**(另一层顶上) | 我变异验证时发现 |
| 3 | 两级权限的"先红后绿"测试关的是**全局能力开关**,那种场景下读不读项目层**答案一样** | 我变异验证时发现 |

三次都是**变异验证**抓出来的,不是看绿抓出来的。

🔴 **验收动作:拿到 Worker 的绿,先想"如果实现是错的,这条测试会红吗",
然后真的去改坏它试一次。** 这比读报告有用得多。

---

## 3. 订阅登录 —— 结论已定(用户已确认)

| 供应商 | 订阅登录 | 结论 |
| --- | --- | --- |
| **xAI** | ✅ 已可用 | 用「已登录的 Grok」(`grok_login`,复用 grok CLI 会话)。**不要再做 OAuth,那是重复建设** |
| **GitHub Copilot** | 🟡 可解 | 官方要求第三方自建应用,但**个人账号即可、免审核免费**。等用户注册后给 client_id → 目录改 `UserSupplied` |
| **Anthropic** | ⛔ **官方禁止** | 条款明令禁止第三方提供 Claude.ai 登录或转发订阅凭据。**应钉死 `ApiKeyOnly`** |
| **OpenAI** | ⛔ 无第三方路径 | 找不到"注册一个能 Sign in with ChatGPT 的客户端"的后台 |

**用户拍板:Claude 和 Codex 走「本机 CLI」槽位。**
让官方客户端用它自己的登录,Pillow 不碰凭据。

🔴 **限制要讲清楚**:本机 CLI 槽位 `SlotExecutionPath::NotRunnable`,
**不能当活对话运行时,只能当派单目标**,而且 `governance: none`
—— 审批链管不到它内部做什么。派单界面上有标注,别把这条抹掉。

调研全文:`docs/research/2026-08-13-oauth-client-ids.md`
(动态注册 RFC 7591 四家全不通,discovery 里没有 `registration_endpoint`。)

---

## 4. 并发语义已拍板(方案 B)

`docs/decisions/2026-08-13-concurrency-semantics.md`

**Pillow 不承诺多槽位同时运行。** 槽位是长期保留的配置,
**同一时刻一个在跑活对话**,其他角色靠受治理的派单。

🔴 **任何文案不得再说「同时工作」/「并行运行」。**

背景值得记:我整轮把"槽位可共存、同时工作"当事实反复陈述,还写进派单简报
当不可破条款 —— 而代码是 `max_concurrent_turns = 1`、切槽位关掉上一个。
**是 Worker 为别的目的读代码时发现的,不是我自查发现的。**

被否决的方案 A(宿主持有 N 个运行时)代价不在起进程,在**审批仲裁**;
将来要做先解决仲裁。

---

## 5. 权限模型:今天从"假的"变成真的

**之前**:设置页展示两级 Bypass,但宿主里
`get_permission_policy` / `set_bypass_available` / `set_project_permission_mode`
**三个命令都不存在**,网关兜住异常、用单个全局值**编造**了一份策略,
所以页面看着完全正常。执行点只读全局。

**现在**:命令补齐并注册,存储有项目维度,两个执行点读**合成值**
`有效 = 全局能力 ∧ 项目选择`。渲染层那个 fallback 删了。

🔴 **老库迁移**:`mode=bypass` 且无显式能力标志 → 推断能力开,
**不静默关掉用户已有的 Bypass**。

同轮还收窄了**过宽 allow**:从 `node build.js` 点「总是允许」不再能存下
`node *`;对话框会用当前 subject 算出**真实的对比例子**。
deny 侧不变(收窄 deny = 漏掉该拒的)。

---

## 6. 🔴 槽位路由(8-12 手册里这段是错的)

| 槽位 | 路由 | 状态(8-13) |
| --- | --- | --- |
| **grok1** | 🔴 **不设环境变量,就是默认 profile** | ✅ 可用 |
| grok2 / grok3 | `USERPROFILE` 与 `HOME` 指向 `<槽位 home>` | ✅ 可用 |
| grok4 | 同上 | ⛔ **402,今天跑满** |
| grok5 / grokbot | — | ❌ 未认证 |

RUNBOOK 第 108 行早写了 grok1 是默认 profile,**我没照做**,给它套了环境变量
→ 指向空目录 → 报"未认证" → 我据此写下"grok1 和 grok3 是同一账号已耗尽"。
**这个错误传了一整天,让我少算两个可用槽位。**

🔴 RUNBOOK 还要求:**解读登录/额度/Worker 结果前,先通过 Keeper 运行时元数据
验证路由。** Keeper 服务不在跑时做不到,退而用 `grok inspect` 核对实际解析到的
config 路径 —— 但要知道那只是替代证据。

**Codex**:`CODEX_SLOT=main`(缺省)= `gpt-5.6-sol`,**不要设 `CODEX_MODEL`**。
🔴 `codex_run.sh` 的 `--cd` 写死在 `<workspace>/denglou-ui-v2`,派 Pillow 的活不能直接用。

---

## 7. 并行派单的纪律

🔴 **前提是文件集不相交,不是槽位有空。**

今天两次教训:

1. 稳定性测量(30 轮 `--lib`)和**同 crate** 的改动并行 → 编译失败混进测量
2. 我把 `Workbench.tsx` 排除在简报允许范围外 → 组件建好了**没人接线**,
   建好即死。Worker 做法没错,**是我边界画错了**

有效的切法:一单**实现**、一单**只写文档的设计/调研**,由构造保证不相交。

---

## 8. 🔴 下一步(按顺序)

1. **把 Anthropic 钉成 `ApiKeyOnly`** —— 用户已确认。
   一行目录数据(`crates/runtime-slots/src/provider_login.rs`)+ 一条测试。
   **本轮没做,是因为用户要压缩上下文、我优先写这份交接。**
2. **跑一轮 E2E** —— 这几轮 UI 改动(设置页新流程、派单可见、活槽位标记、
   权限两级)**只有单测覆盖**。E2E 约 30 分钟、总控独占。
3. **重建二进制**:`cd apps/desktop && corepack pnpm tauri build --no-bundle`
   🔴 **必须走 Tauri CLI。** 裸 `cargo build --release` 产出的 exe 指向 devUrl、
   前端不嵌入,开出来是 `127.0.0.1 refused to connect`。我交过一次这样的二进制。
   验证:`grep -ac "<!doctype html>" target/release/pillow.exe` 应为 1。
   🔴 **重建会替换 exe,先跟用户说一声**(运行中的实例会锁住文件)。
4. 🔴 **用户跑一轮真实任务** —— 见 `docs/runbooks/first-real-task.md`
5. **槽位级工具权限**(设计已完成:`docs/decisions/2026-08-13-per-slot-tool-permissions-design.md`)
   —— 最小切片是槽位 deny-only + run 绑 `slot_id`。**等两级权限稳定后再做。**
6. **打包**(用户明确:最后一轮)

---

## 9. 环境

- 门:`node scripts/run-worker-gate.mjs` —— **唯一合规入口**
  🔴 **后台任务报的 exit code 是那条 bash 的,不是门的。必须读日志里的 `GATE_EXIT`。**
- E2E:`cd apps/desktop && corepack pnpm test:e2e`,约 30 分钟,总控独占
- 🔴 日志写 `.superpowers/logs/`,**不要写 `/tmp`**(另一个 Claude 进程会清)
  也不要写裸文件名 —— 会落在仓库根目录
- 🔴 **node 里 `/tmp` 解析成 `F:\tmp`**,和 bash 的 `/tmp` 不是同一个地方
- 🔴 **Bash 工具的工作目录会跨调用保留**,`cd` 之后记得用绝对路径
- 台账:`knowledge-indexer task dispatch --db .superpowers/ledger/pillow.db ...`

---

## 10. 今天修掉的三个假红生成器

1. **WMI 快照 3 秒硬超时、零重试** —— 一次瞬时超时废掉整轮 E2E。已加重试
   (按尝试次数判退出,不按延迟表),并**同步修了 git-changes harness 里逐字重复的那份**
2. **源码契约测试撞 CRLF** —— 断言的是文件字节表示,而那取决于 git 怎么检出。
   已在匹配前归一化
3. **C1 视觉验收预算只剩 10% 余量**(126s 顶 150s)—— 已放宽到 300s,
   并把实测数字写进注释,好让以后能区分"真变慢"和"余量不够"
