# Wave C2-E CDP 驱动修复 Brief

## 身份与边界

- 项目：`<workspace>\pillow`
- 角色：Grok Worker；先通过 Keeper boot，`project=pillow`、`agentRole=worker`
- 基线 HEAD：`7b7f70f7360131cd1cfd984a8b19bdfb5725c21b`
- 工作树包含 C1/C2 已验收但未暂存、未提交的改动。不得丢失、覆盖、回退、暂存或提交。
- `<workspace>\VaultSystem\keeper` 是冻结旧资源管理器，只读。

## 当前失败证据

总控亲跑：

```powershell
node --test --test-concurrency=1 apps/desktop/tests/e2e/c2-resource-workbench.spec.ts
```

调试构建成功；第一个 `register-a` 的真实 Tauri 注册证据已写出，应用仍存活，但：

```text
KEEPER_WAVE_C2_RESOURCE_ACCEPTANCE
status=failed
error=CDP endpoint on port 65156 not ready: fetch failed
```

失败发生在 `scripts/wave-c2-resource-acceptance.mjs` 的
`connectWebViewPage()` / `waitForCdpEndpoint()`。当前通过
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<dynamic-port>`
尝试开启 WebView2 CDP。

## 目标

修复测试专用 WebView2/CDP 接入，使 C2-E 四阶段真实桌面验收能够运行：

1. register-a
2. register-b
3. create
4. restore

必须保持真实 Tauri/WebView2、临时 app data、临时 WebView2 profile、正常窗口关闭和零残留进程证明；不得降级为浏览器 mock、直接 Tauri invoke、直接 SQLite 或直接文件系统断言。

优先诊断实际启动的 WebView2 子进程参数、监听端口与 profile 中的调试端点信息。允许采用官方支持的 test-scoped CDP 方式，例如固定可用端口或 `--remote-debugging-port=0` 配合实际端点发现，但不得写注册表、不得修改机器级策略。

总控已确认当前 Codex/测试进程为 High Mandatory Level。在此环境下，直接传入
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` 未产生 CDP 监听，且
`DevToolsActivePort` 不存在。

禁止使用计划任务、`runas`、Explorer ShellExecute、注册表、组策略或任何机器级
持久项来降权/注入参数；禁止在仓库中创建临时诊断脚本。

优先验证安全的构建期测试配置覆盖：Tauri CLI `--config <acceptance-owned-temp-json>`
为唯一调试构建注入 WebView2 `additionalBrowserArgs` 和独立 `dataDirectory`，
四阶段串行复用固定且预先确认空闲的 CDP 端口；临时配置必须位于 acceptance-owned
临时目录并在零残留进程后删除。若本地 Tauri schema/API 不支持这一方案，停止并在
报告中给出证据，不要扩大机器权限或修改生产代码。

## 写入白名单

- `scripts/wave-c2-resource-acceptance.mjs`
- `apps/desktop/tests/e2e/c2-resource-workbench.spec.ts`
- `docs/execution/2026-07-31-wave-c2e-orchestrator-report.md`
- `docs/execution/2026-07-31-wave-c2e-cdp-driver-fix-report.md`

禁止修改任何生产 Rust、React、CSS、Cargo 或 package 文件。

## 验证

至少运行：

```powershell
node --check scripts/wave-c2-resource-acceptance.mjs
pnpm --filter @pillow/desktop typecheck
git diff --check
```

如果环境与时间允许，再运行聚焦 E2E；无论结果如何，都必须留下完整、可复核的失败/通过终端记录。禁止通过放宽断言或吞错取得绿灯。

## 交付

完成时：

1. 写 `docs/execution/2026-07-31-wave-c2e-cdp-driver-fix-report.md`
2. 报告 changed files、根因、验证命令、残余风险
3. 通过 Keeper completion message 发送完成信号
4. 清理自己启动的所有进程
5. 不 stage、不 commit
