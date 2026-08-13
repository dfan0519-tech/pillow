# Pillow — 两级权限落到宿主

**Date:** 2026-08-13  
**Worker:** grok1 · **总控:** Claude  
**完成信号:** `TWO-TIER-COMPLETE`

把「有效模式 = 全局允许 Bypass ∧ 项目选择」从渲染层 fallback 落到存储、宿主命令和执行点。deny 始终优先；配置损坏 fail-closed 到每次询问。

## 退出码表

| 命令 | 日志 | EXIT |
| --- | --- | --- |
| `node scripts/run-cargo.mjs fmt --all -- --check` | `.superpowers/round50/tt-fmt.txt` | 0 |
| `node scripts/run-cargo.mjs test -p keeper-storage-sqlite` | `.superpowers/round50/tt-log1.txt` | 0 |
| `node scripts/run-cargo.mjs test -p keeper-runtime-host` | `.superpowers/round50/tt-log2.txt` | 0 |
| `node scripts/run-cargo.mjs test -p pillow-desktop --lib` | `.superpowers/round50/tt-log3.txt` | 0 |
| `corepack pnpm --filter @pillow/desktop test` | `.superpowers/round50/tt-log4.txt` | 0 |

先跑了 `cargo fmt --all`（EXIT 0），再记 `--check`。收口四组测试均为 0。未跑仓库门、未跑 E2E。

## 先红后绿

**测试**

- `crates/runtime-host/tests/governed_write_bridge.rs`  
  `global_bypass_off_keeps_project_bypass_write_pending_human_approval`
- `crates/runtime-host/tests/governed_process_bridge.rs`  
  `global_bypass_off_keeps_project_bypass_process_pending_human_approval`

**夹具（旧执行点会读错的形状）**

1. 全局 `permissions.mode = bypass`（旧 `resolve_permission_mode()` 只看这一行）
2. 项目 `permissions.project_mode = bypass`
3. 再把全局 `permissions.bypass_available` 写成 `false`

**旧代码上的失败（执行点尚未改读合成模式）**

日志：`.superpowers/round50/tt-red-write.txt`

```
test global_bypass_off_keeps_project_bypass_write_pending_human_approval ... FAILED
called `Result::unwrap()` on an `Err` value: DecisionDenied
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 28 filtered out
```

原因：旧路径只读全局 preferred。全局仍是 `bypass`，写入被模式自动批准；测试线程再去 `decide(Reject)`，提案已经不是人工待批，得到 `DecisionDenied`。这就是「执行点只看全局」的红。

**改完之后**

`governed_write.rs` / `host/process.rs` 改为  
`resolve_effective_permission_mode(Some(project_id))`。  
同一测试在 `tt-log2.txt` 中为 `ok`。能力关闭时不再发模式收据，必须露出人工 grant / `decision_available`。

## 合成规则判定表

`effective = chosen.gated_by_bypass_capability(bypass_available)`  
`chosen`：有项目 id 时取项目选择（无行则种子自安装 preferred）；无项目 id 时取 preferred。  
Bypass 且能力关闭 → **始终** `default`（每次询问），不会落到 `accept_edits`。

| 全局 Bypass 可用 | 项目 / 所选模式 | 有效模式 |
| --- | --- | --- |
| 关 | `default` | `default` |
| 关 | `accept_edits` | `accept_edits` |
| 关 | `bypass` | **`default`** |
| 开 | `default` | `default` |
| 开 | `accept_edits` | `accept_edits` |
| 开 | `bypass` | `bypass` |

钉在 `two_tier_composition_table_global_and_project`。deny 规则仍在模式之前求值；本单未改 `kernel-policy`。

## 迁移：老库的 `permissions.mode`

**选择**

- `permissions.mode` **继续**是安装级 preferred，且仍只能全局写（SQLite trigger `trg_settings_permission_mode_global_*` 不能在本单改，项目级必须用新键）。
- 新键 `permissions.bypass_available`（全局 flag）：显式 Bypass 能力。
- 新键 `permissions.project_mode`（仅项目）：该项目批准模式。

**缺省 flag 时的解释（不静默丢设置）**

| 老行 `permissions.mode` | 推断能力 | preferred | 无项目行时的有效模式 |
| --- | --- | --- | --- |
| 无行 | 关（fail-closed） | `default` | `default` |
| `default` | 关 | `default` | `default` |
| `accept_edits` | 关 | `accept_edits` | `accept_edits`（写自动批仍在） |
| `bypass` | **开** | `bypass` | `bypass` |
| 坏枚举 | 关 | `default` | `default` |

理由：老模型是单轴「此刻生效的模式」，不是「能力开关」。用户已经选了 `bypass` 就必须在升级后仍能 Bypass；选了 `accept_edits` 必须仍自动批写。不能把整行删掉，也不能把 `accept_edits` 误读成「能力开且模式 default」。  
显式 `permissions.bypass_available = false` **压过**「preferred 仍是 bypass」的推断：preferred 行保留（不丢），有效模式落到 `default`。见 `legacy_permissions_mode_is_not_discarded`、`explicit_bypass_off_overrides_legacy_bypass_row`。

项目不能写能力键；全局不能写 `permissions.project_mode`；项目在能力关闭时 `upsert_project_permission_mode(bypass)` 被拒（`permission_bypass_not_available`）。已有项目 bypass 行在能力被关掉后仍留在库里，但合成后不生效。

遗留 `set_permission_mode` 仍是安装级：打开 Bypass 时顺带把能力写成开，避免老「一轴」写入失去 Bypass。它**没有**项目 id，打开的项目不能靠这条给自己提权。

## 渲染层 fallback

删掉 `synthesizePolicyFromLegacy` 以及 `getPermissionPolicy` 里「宿主没有命令就用全局模式编一份策略」的路径。

现在：

- 宿主答得上 → 闭合五键校验（`bypassAvailable` / `bypassAvailableSource` / `projectMode` / `projectModeSource` / `effectiveMode`）。多一个键就拒。
- 宿主声称 `effectiveMode=bypass` 但 `bypassAvailable=false` → 解析层再闸到 `default`。
- 命令不存在 / `settings_command_failed` → `permission_policy_host_missing`，设置页走已有 error 条，不再假装策略正常。

钉在 `apps/desktop/tests/settings-permission-policy-gateway.test.ts`。Rust DTO 与渲染层同批改；`permission_policy_dto_accepts_exact_keys_and_gates_project_bypass` 钉死序列化键集；`desktop_invoke_handler_registers_two_tier_permission_commands` 钉死 `generate_handler!`。

## 残留风险

1. **工作台指示器仍读 `get_permission_mode`（preferred，不是合成有效模式）**。能力已关、preferred 仍是 bypass 时，指示器可能显示 Bypass，设置页则显示有效 `default`。本单不改工作台组件。
2. **`set_permission_mode("default")` 不清理各项目的 `permissions.project_mode`**。工作台「恢复默认」只改安装 preferred。
3. **没有新的 SQL migration 文件**（不在允许改动清单）。新键走已有 `settings_entries`；`permissions.mode` 的全局-only trigger 仍在，所以项目模式必须用另一把键。
4. **无项目上下文的执行不会发生在当前治理桥**（绑定 run 总有 project id）。若未来有无项目调用，`resolve_effective_permission_mode(None)` 只闸 preferred，不读项目行。
5. **坏 flag / 坏项目模式 fail-closed 到「能力关 / default」**，不报错给 UI。和现有 `resolve_permission_mode` 对坏枚举的精神一致；操作员只能靠「为什么 Bypass 没了」排查。
6. **推断能力依赖「preferred 仍是完好的 bypass 行」**。若有人先把 preferred 改成别的、又从没写过 flag，能力会变成关。显式 flag 一旦写过就不再走推断。
7. **设置页在宿主缺失时会显示错误而不是假策略**。这是本单要求；对尚未升级的宿主二进制是可见回归。

TWO-TIER-COMPLETE
