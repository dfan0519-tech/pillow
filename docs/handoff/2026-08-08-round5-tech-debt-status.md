# Round 5 技术债收口 — 状态记录(暂停点)

日期:2026-08-08
控制者:Claude(总控)
分支 / HEAD:`main` / `a2fda9c`(本轮改动尚未提交时的基线)
性质:C4 checkpoint 之后的技术债收口,**未开始任何新能力**

## 1. 一句话状态

C4 已 checkpoint;本轮收掉了 checkpoint 记录的多数未解决风险,
`pnpm check` 与**连续两次**完整串行桌面 E2E 全绿(10/10、10/10)。
`cancel-tree` 根因仍未知,生命周期事件实施待做。

## 2. 本轮完成的事

四个 Grok 槽位并行(grok1/2/3/4,四个独立账号,文件簇互不相交),
外加总控自己的修复与验收。

| 项 | 承担 | 结果 |
| --- | --- | --- |
| 面板标签三重冗余(checkpoint §5.4) | Worker A / grok3 | 已修 |
| 生命周期事件观测缺口(§5.3) | Worker B / grok1 | **正确 BLOCKED**,交回已采纳设计 |
| Git fixture 拆绕行 + 环境隔离 | Worker C / grok2 | 已改 |
| `cancel-tree` RED 捕获(§5.1/5.2) | Worker D / grok4 | **未复现**(8 次),取证已加强 |
| 未跟踪目录缺陷第 2、3 层 | 总控 | 已修 |
| E2E vctip 清理竞态 | 总控 | 已修 |
| fixture 提交吞掉 `vendor/` | 总控 | 已修 |

## 3. 本轮最重要的发现:一条规则被独立实现了三遍

「可移植路径」这条不变量在三处各写了一份,且都用同样的
「按 `/` 切分、每段非空」逻辑,于是 porcelain v2 的合法目录记录 `? vendor/`
在每一层都被拒:

| 层 | 位置 | 修复时间 |
| --- | --- | --- |
| 解析器 | `crates/git-executor/src/status.rs` | Round 4 |
| 项目路径映射 | `crates/git-executor/src/discovery.rs` | Round 5 |
| 渲染层 DTO 校验 | `apps/desktop/src/lib/tauriGitGateway.ts` | Round 5 |

**这是架构债,不是三个 bug。** 它导致连续两轮出现「以为修好了」:
Round 4 我把它记为 Important 已修复,实际只修了第一层;
而验收之所以没抓到,是因为 acceptance fixture 用 `.gitignore` 绕开了它
—— 那个绕行正是 Round 3 worker 为规避该缺陷而加的。
Worker C 拆掉绕行后,三层才依次暴露。

**建议下一步**:把可移植路径规则收敛成单一事实源,并用跨层契约测试锁住。
在那之前,任何「某处路径校验」的改动都要三层同查。

现状是三层已对齐,各带回归测试:
- Rust 单元 + 集成(含真实 git 的根仓库嵌套仓库用例)
- 渲染层 DTO 测试,并明确钉住**目录形式不得作为 diff 目标**

## 4. 顺带消除的一类 E2E 抖动

`C2-E` 曾挂在 `Desktop cleanup refused: observation-only PIDs remain`,
与早先 `run-cargo.mjs` 的问题同源:MSVC 分离的 `vctip.exe` 遥测进程
存活约 6 秒,正好卡在 `CLEANUP_ATTEMPTS = 120 × 50ms` 的宽限边界。

7 个 E2E spec 里只有 4 个较新的带 `allowMsvcVctipTelemetry`,现已全部补齐。
该豁免身份门控严格(精确 MSVC 路径正则 + 严格 `=== true` + 仅在 root 确认死亡后),
未削弱任何清理逻辑。

这是「E2E 又慢又脆」这条债里**第一块有确定解释并已消除**的部分。

## 5. 仍然开放的问题

### 5.1 `cancel-tree` / `two-project-isolation` 根因未知(最高优先)

Worker D 用独占 E2E 通道跑了 8 次,**0 次复现**,并诚实拒绝编造根因。
排除性证据:7 次绿路径中 `composer-cancel` 全程可见,
approve→cancel 稳定 600–780ms,child markers 恒为 2
—— 排除「approve 后 cancel 闪断」这个最直觉的假说。

已就位的取证(下次 RED 会自动落盘到 `.superpowers/c4-cancel-tree-debug/`):
- cancel-tree 相位时间线(绿/红都存)
- `waitForCommandApproval` 失败 dump + 相位标签 + 截图

**处置**:不要在拿到 RED 之前改产品 cancel/approval 路径。
下次完整串行套件失败时,先读 dump。

### 5.2 生命周期事件实施(§5.3)

设计已由 Worker B 完成并经总控审核采纳,**未施工**。
简报已写好:`.superpowers/round5/BRIEF_B2_lifecycle_impl.md`(gitignored)。

要点:发射编排移到 `runtime-host` waiter 线程既有回调链;
`decide` 只标记资格、不持有 EventSink;
`started` 必须在 `claim_run` 返回 `Claimed` 之后;
不新增事件种类、不改阻塞模型、replay 门与持久化顺序不变。

白名单已批准扩到 `runtime-host` + `tool-executor` + adapter。

### 5.3 其它

- 模式切换器用内联 `style={{}}` 而非 `execution.css`(上一轮遗留,未处理)
- `docs` 261 个 vs 代码 216 个文件,治理开销偏重(未处理)
- 完整串行 E2E 约 20 分钟,仍是唯一能拦住集成级回归的网(未分层)

## 6. 环境注记

- `pnpm` 不在本机 bash PATH。一个 Worker 自行跑了 `corepack enable`,
  把 pnpm shim 装进了 `C:\Program Files\nodejs\`(仓库之外的系统改动,已记录)。
- Grok 槽位:1/2/3/4 四个独立账号容器,本轮全部用过,租约均已归还。
  槽位路由每次都按会话落点亲验(`<容器>/.grok/sessions/<编码cwd>/`)。

## 7. 暂停点的门状态

| 门 | 结果 |
| --- | --- |
| `pnpm check` | exit 0 — renderer **374/374**、Rust workspace、Clippy `-D warnings` |
| 完整串行桌面 E2E ×2 | **10/10、10/10** |
| `cargo fmt --all --check` | exit 0 |
| `git diff --check` | exit 0 |
| `scripts/check-secrets.mjs` | 9 条模式自检通过,无发现 |
| 残留 `pillow.exe` / 1420 | 0 / 0 |
| Grok 在途 Worker | 0 |

## 8. 明天从哪继续

建议顺序:

1. 派 B2 施工生命周期事件(简报已就绪,设计已定死)。
2. 决定是否把可移植路径规则收敛成单一事实源(§3),这是本轮暴露的最大结构性问题。
3. `cancel-tree` 保持观察态,等 RED 自己出现,不主动改产品。
4. 之后才谈 E2E 分层提速与文档瘦身,再之后是 D 阶段 Runtime 扩展
   —— D 开工前建议先把治理结果类型从 `runtime-host` 搬到 `agent-protocol`,
   否则每个新 adapter 都要依赖整个宿主 crate(架构审视结论)。

新任务开工前需要设计授权。
