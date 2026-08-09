# Wave C2-E 总控拒收修复 Brief

## 身份与边界

- 工作目录：`this repository`
- 先 Keeper boot：`project=pillow`、`agentRole=worker`
- HEAD 必须保持 `7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
- 不得 stage、commit、reset、回退或覆盖既有 C1/C2 工作树。
- `legacy Keeper (out of tree)` 冻结只读。
- 禁止计划任务、runas、Explorer ShellExecute、注册表、组策略和机器级持久项。

## 当前状态

构建期 Tauri `additionalBrowserArgs` 已使 CDP 接入通过 register-a/register-b，
但总控拒收当前实现：

1. create 阶段在搜索结果按钮 locator 超时：
   `getByRole("button", { name: /notes\/a-only\.md/ })`
2. `closeDesktopNormally` 删除了对 `ProcessSupervisor.knownPids` 的严格零残留检查，
   并在注释中宣称孤儿 WebView2 utility PID 可忽略。
3. 验收构建覆盖仓库 `target/debug/pillow.exe`，会留下烘入的临时 CDP 端口。
4. 仅凭 readiness 中的环境变量不足以证明 WebView2 实际使用临时 profile。

## 目标

完成真实 C2-E 四阶段 E2E，且不放宽门禁：

- register-a
- register-b
- create
- restore

## 写入白名单

- `scripts/wave-c2-resource-acceptance.mjs`
- `scripts/lib/c2-resource-ui-acceptance.mjs`
- `apps/desktop/tests/e2e/c2-resource-workbench.spec.ts`
- `docs/execution/2026-07-31-wave-c2e-cdp-driver-fix-report.md`
- `docs/execution/2026-07-31-wave-c2e-controller-rejection-fix-report.md`

禁止修改生产 Rust、React、CSS、Cargo、package 文件。
禁止在仓库创建临时脚本或测试日志。

## 强制修复要求

### A. 搜索结果 locator

读取真实 `ResourcePane` DOM/ARIA。搜索结果已公开渲染完整 portable path；
使用稳定的公开 DOM/accessible contract 定位包含 `ownPath` 的按钮。
不得绕过点击、直接调用 gateway、Tauri invoke、SQLite 或文件系统来伪造预览。

### B. 严格零残留

恢复并加强 `ProcessSupervisor.knownPids` 检查：

- 点击公开“关闭”按钮；
- root `pillow.exe` 必须 status 0、无 signal；
- 在有界 grace period 内等待所有已捕获受管 PID 自然退出；
- grace period 后任一受管 PID 存活，则执行 safety-net cleanup 并让验收失败；
- CDP port 必须释放；
- 不得把 orphan WebView2 utility PID 声明为可忽略；
- 强制 cleanup 永远不能计为通过。

### C. 不污染仓库 debug 二进制

验收调试构建必须设置 acceptance-owned 临时 `CARGO_TARGET_DIR`，并从该目录启动
`pillow.exe`。测试结束且进程清零后删除临时 target。不得覆盖或留下
`this repository\target\debug\pillow.exe` 的 CDP 配置。

### D. 临时 WebView2 profile 证明

必须从实际 WebView2 browser 进程命令行或同等真实运行证据证明其 user-data-dir
位于 acceptance-owned 临时 profile。环境变量回显本身不算证明。
若当前 High IL 环境无法获得临时 profile，停止并报告，不得弱化真实 profile
不变门禁。

## 验证

```powershell
node --check scripts/wave-c2-resource-acceptance.mjs
node --check scripts/lib/c2-resource-ui-acceptance.mjs
pnpm --filter @pillow/desktop typecheck
git diff --check
node --test --test-concurrency=1 apps/desktop/tests/e2e/c2-resource-workbench.spec.ts
```

必须保留稳定终端记录。E2E 红则 `DONE_WITH_CONCERNS` 或 `BLOCKED`，禁止报完成。

## 交付

- 写 `docs/execution/2026-07-31-wave-c2e-controller-rejection-fix-report.md`
- 报 changed files、测试、残余风险
- Keeper completion/blocker 信号
- 清理全部自有进程与临时目录
- 不 stage、不 commit
