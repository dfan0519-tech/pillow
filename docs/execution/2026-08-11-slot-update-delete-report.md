# Pillow — 槽位更新与删除（补宿主缺口）实施报告

状态：实现完成；聚焦门绿；全仓 desktop clippy / 总门受并发与既有 secrets 夹具影响，分列如下。

Worker：grok1（写码）。总控：Claude。

## 0. 范围与硬边界

| 项 | 结论 |
| --- | --- |
| 允许写入 | `crates/storage-sqlite/src/runtime_slot.rs` + tests、`crates/runtime-slots/**`、`apps/desktop/src-tauri/src/runtime_slot_commands*.rs`、本报告 |
| 额外最小改动 | `apps/desktop/src-tauri/src/runtime_service_state.rs`：新增 `shutdown_if_active_slot`，删除活跃槽位前先停 runtime（见 §3） |
| 禁止写入（已遵守） | **`lib.rs` 零改动**、`apps/desktop/src/**`、`crates/git-executor/**`、`git_commands*.rs`、`adapter-grok-acp` RPC methods、`scripts/**` |
| 密钥 | 不进 DTO / 日志 / 错误 / Debug / 本报告；测试 canary 字符串仅为本地夹具，非真实密钥 |
| 依赖 | 未新增第三方包；`Cargo.lock` 无新 package |
| 门控 | 隔离 target 目录相对仓库为 `target-slot-crud`；禁 desktop E2E；禁 git 写 |

## 1. 退出码表

所有命令均直接执行，未通过管道截断输出。

| 命令 | 退出码 | 结果 |
| --- | ---: | --- |
| `cargo test -p keeper-storage-sqlite --test runtime_slot_repository --locked --offline` | 0 | PASS；8/8（含 update/delete 3 项新测） |
| `cargo test -p pillow-runtime-slots --locked --offline` | 0 | PASS；9/9（含 omit-keep / rotate / delete 实质断言） |
| `cargo test -p pillow-desktop --lib runtime_slot_commands_tests --locked --offline` | 0 | PASS；11/11（含 omit-keep+start、rotate、delete+停活跃） |
| `cargo clippy -p keeper-storage-sqlite -p pillow-runtime-slots --locked --offline -- -D warnings` | 0 | PASS |
| `cargo clippy -p pillow-desktop --locked --offline -- -D warnings` | 101 | **PEER-WIP / 非本单**：`keeper-git-executor`（codex1）`private_interfaces` / unused import；本单未改该 crate |
| `cargo test -p keeper-adapter-grok-acp --lib debug_output_redacts_the_dedicated_api_key --locked --offline` | 0 | PASS；既有 redaction 仍绿 |
| `git diff --exit-code -- crates/adapter-grok-acp/src/rpc/methods.rs` | 0 | PASS；ACP 边界零差异 |
| `cargo fmt`（仅本单文件列表） | 0 | PASS |
| `corepack pnpm check` | 1 | **ENV-BLOCKED / 非本单**：secrets 门在渲染层 UI 单测夹具命中 openai-key 模式（`runtime-slots-gateway.test.ts` / `runtime-slots-ui.test.tsx`）；本单未改 `apps/desktop/src/**` 与 `scripts/`，未绕过 |

## 2. `lib.rs` 需要新增的注册行原文（总控合并）

**未改** `apps/desktop/src-tauri/src/lib.rs`。请总控合并下列改动（命令符号已在 `runtime_slot_commands.rs` 中公开存在）。

### 2.1 `use` 列表

将现有：

```rust
use runtime_slot_commands::{
    create_external_cli_slot, create_grok_api_key_slot, create_grok_login_slot,
    get_external_cli_status, list_runtime_slots, start_runtime_slot,
};
```

替换为：

```rust
use runtime_slot_commands::{
    create_external_cli_slot, create_grok_api_key_slot, create_grok_login_slot,
    delete_runtime_slot, get_external_cli_status, list_runtime_slots, start_runtime_slot,
    update_grok_api_key_slot,
};
```

### 2.2 `generate_handler!` 片段

在现有槽位命令旁增加两行（建议紧接 `create_grok_api_key_slot` 之后、`create_external_cli_slot` 之前，或列尾均可）：

```rust
        list_runtime_slots,
        create_grok_login_slot,
        create_grok_api_key_slot,
        update_grok_api_key_slot,
        delete_runtime_slot,
        create_external_cli_slot,
        get_external_cli_status,
        start_runtime_slot
```

合并后请删除 `runtime_slot_commands.rs` 上两个 `#[allow(dead_code)]`（仅为 lib 注册前的过渡抑制）。

合并后建议扩展既有测试 `desktop_commands_use_replaceable_runtime_state_and_register_slot_surface` 的命令列表，加入 `update_grok_api_key_slot` 与 `delete_runtime_slot`。

## 3. 命令契约

### 3.1 与渲染层一致性

| 命令 | 与 `slots-ui-report` 期望 | 说明 |
| --- | --- | --- |
| `update_grok_api_key_slot` | **一致** | 渲染层 `TauriRuntimeSlotsGateway` 已 invoke 该名 |
| `delete_runtime_slot` | **新增**（UI 报告曾记「无删除命令」） | 宿主先补齐；渲染层后续可接删除入口 |

### 3.2 输入

| 命令 | 输入键（camelCase / Tauri） | 语义 |
| --- | --- | --- |
| `update_grok_api_key_slot` | `projectId`, `slotId`, `displayName`, `baseUrl`, `modelId`, `apiKey?` | **省略 / null / 仅空白 `apiKey` ⇒ 保持原密钥**；非空 ⇒ **轮换** |
| `delete_runtime_slot` | `projectId`, `slotId` | 删除任意 kind；API-key 槽位一并销毁 DPAPI 密文 |

响应仍为既有 `RuntimeSlotDto`（update）或 `()`（delete）。DTO 无 `apiKey`；仅 `secretRef` + `secretConfigured`。

### 3.3 密钥与轮换行为

1. **省略 apiKey**：`secret_ref` 不变；DPAPI 文件不动；`prepare_grok` 仍解出原明文（测试证明「可用」，非仅 Ok）。
2. **提供新 key**：新 opaque `secret_ref` → 先 `put` 新密文 → 元数据改指新 ref → **删除旧密文文件**；旧 ref `get` → `SecretMissing`；`prepare` 解出新明文。
3. **删除**：若该槽位是进程内活跃 runtime，**先 `shutdown_if_active_slot`**，再删密文、再删元数据、再 best-effort 清理私有 home。
4. 元数据更新失败且已写新密文时：回滚删除新密文，避免不可达密文。

### 3.4 独立探活

**未做。** 现有 `start_runtime_slot` 仍兼探活/激活。低成本只读网络 canary 需稳定供应商错误码与额外依赖/端点约定，本单不硬做。

## 4. 正反测试（非假绿）

| # | 要求 | 测试 | 实质断言（非仅「没报错」） |
| --- | --- | --- | --- |
| 1 | 省略 apiKey → 原密钥保留且仍可用 | service `update_without_api_key_keeps_the_original_secret_usable`；命令 `update_omitting_api_key_keeps_secret_usable_for_prepare_and_start` | `secret_ref` 不变；`store.get(ref)` 字节等于原 canary；`prepare_grok` 返回同字节；命令层还能 `start_runtime_slot` → `active` |
| 2 | 新 key → 轮换；旧密文不可再解 | service `update_with_new_api_key_rotates_and_destroys_prior_ciphertext`；命令同名 | 新 ref ≠ 旧 ref；旧 `.dpapi` 文件不存在；`get(old_ref)` → `SecretMissing`；`get(new_ref)` / `prepare` 等于新 canary |
| 3 | 删除 → 密钥确实消失 | service `delete_destroys_slot_metadata_and_secret_ciphertext`；命令 `delete_command_removes_slot_and_destroys_secret` | list 空；密文文件不存在；`get(ref)` → `SecretMissing`；`prepare` → `runtime_slot_not_found`；删除活跃槽后 `shutdown_if_active` 为 false |
| 4 | DTO 永不含密钥 | 既有 `serialized_slot_dto_has_an_exact_allowlist_and_never_contains_the_key` + update 序列化断言 | JSON 无 canary、无 `apiKey` 键 |
| 5 | 仓库层 update/delete | `update_grok_api_key_*` / `delete_removes_the_row_and_is_project_scoped` | 元数据字段与 secret_ref 更新；跨项目 delete 无效；二次 delete `None` |
| 6 | ACP redaction 回归 | `debug_output_redacts_the_dedicated_api_key` | 仍绿 |

## 5. 写入边界审计

| 路径 | 本单 |
| --- | --- |
| `apps/desktop/src-tauri/src/lib.rs` | **未碰**（注册行见 §2） |
| `apps/desktop/src/**` | **未碰** |
| `crates/git-executor/**`、`git_commands*.rs` | **未碰**（status 中改动属 codex1） |
| `crates/adapter-grok-acp/src/rpc/methods.rs` | **未碰** |
| `crates/storage-sqlite/src/runtime_slot.rs` + tests / lib export | 已改 |
| `crates/runtime-slots/**` | 已改 |
| `runtime_slot_commands*.rs` | 已改 |
| `runtime_service_state.rs` | 最小：`shutdown_if_active_slot` |
| 本报告 | 新建 |

## 6. 残留风险

1. **`lib.rs` 尚未注册**：合并前 renderer invoke `update_grok_api_key_slot` 仍会走「更新暂不可用」类路径；delete 尚无 UI 入口。
2. **desktop 全量 clippy / 总门**：受 codex1 `git-executor` 与渲染层 secrets 夹具影响；本单聚焦包与命令测已绿。
3. **活跃槽位更新不强制重启 runtime**：改 key 后内存中已启动的 runtime 仍持旧配置，直到再次 `start_runtime_slot`。删除会先停活跃槽位。
4. **跨存储非事务**：轮换在「新密文已写、旧密文待删」窗口若进程崩溃，可能短暂双密文；失败路径会回滚新密文。删除优先毁密文再删行，避免孤儿密文优先于「行在密钥不在」。
5. **无私有 home 的强制校验删除**：`remove_dir_all` best-effort；失败不阻断 delete 成功。
6. **无独立网络探活**：与 foundation / UI 报告一致。
7. **过渡 `#[allow(dead_code)]`**：注册合并后应删除。

## 7. 主要文件

- `crates/storage-sqlite/src/runtime_slot.rs` — `UpdateGrokApiKeySlot`、`update_grok_api_key`、`delete`、`NotFound`
- `crates/runtime-slots/src/lib.rs` — service update/delete + 轮换顺序
- `apps/desktop/src-tauri/src/runtime_slot_commands.rs` — `update_grok_api_key_slot` / `delete_runtime_slot`
- `apps/desktop/src-tauri/src/runtime_service_state.rs` — `shutdown_if_active_slot`
- `crates/storage-sqlite/tests/runtime_slot_repository.rs`
- `crates/runtime-slots/tests/slot_service.rs`
- `apps/desktop/src-tauri/src/runtime_slot_commands_tests.rs`

---

**SLOT-CRUD-COMPLETE**
