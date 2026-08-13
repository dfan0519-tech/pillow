# 设计:槽位身份与「从零可用」

日期:2026-08-13
状态:**待维护者确认**
依据:`docs/research/2026-08-13-multi-agent-management.md`(以下简称「调研」)

## 0. 为什么写这份

维护者的原话:

> 你不能默认我设备上已经有 grok cli 了,这里多 agent 管理,以及多 agent 授权的方式,
> 创建到隔离,以及交互,都不完善……**主要的问题是你要让 pillow 从零可以开始做这些事情。**

调研在 8-13 就交了,**结论没有被用到设计上**。它第 4.3 节列了 5 条「我们做错了该换掉的」,
到今天只做了并发语义那一条。这份文档把其中和「身份/授权/隔离」相关的部分补上。

---

## 1. 现状:三处都有隔离,唯独登录槽位漏接

| 位置 | 身份目录 | 代码 |
| --- | --- | --- |
| 设置 →「Grok profile 路径」 | ✅ 全局一个 | `SettingsPanel.tsx` |
| 老的非槽位路径 | ✅ 总设 `honor_isolated_home` | `runtime_composition.rs::finish_grok_config` |
| **API key 槽位** | ✅ 私有目录 + auth.json 污染检查 | `grok_config.rs::write_private_grok_config` |
| **Grok · 登录槽位** | ❌ **什么都不设** | `runtime_composition.rs::build_slot_grok_config` 的 `Login` 分支 |

🔴 **后果:这台机器上所有「Grok·登录」槽位用的是同一个账号。** 建 4 个还是同一个。
维护者有 4 个 grok 账号,在 Pillow 里区分不了 —— 根因就是上表最后一行。

不是缺一套机制,是这一条路径漏接了。

## 2. 现状:「从零」的第一道门是开发者信息

干净机器上没有 grok CLI,用户看到的是:

> `Grok executable not found on PATH. Set PILLOW_GROK_EXECUTABLE or a settings path
> to the Grok CLI, or set PILLOW_RUNTIME_SELECTION=fake for explicit development mode.`

让终端用户去设环境变量,还提到「fake 开发模式」。**这句话不该出现在产品的第一道门上。**

---

## 3. 设计

### 3.1 每个槽位一个身份目录(含登录槽位)

调研 §3.A.1:龙虾的一个 agent = workspace + **`agentDir`(自己的 auth store)** + 自己的会话库。
Pillow 的对应物已经有(`app_data/runtime-slots/<slot_id>`),只是登录槽位没用上。

**动作**:登录槽位创建时也分配私有目录,启动时 `honor_isolated_home` 指向它。

🔴 **和 API key 槽位的污染检查方向相反**:API key 槽位**禁止**出现 auth.json
(带密钥的槽位不该携带登录);登录槽位的 auth.json **就是它的身份**,必须允许。
这两条不能共用一个校验函数,否则一定会互相踩。

### 3.2 授权:在那个目录里登录,Pillow 不碰凭据

调研 §3.A.4:「要完全独立的 OAuth 账号:**在那个 agent 里重新登录**」。

**动作**:槽位上提供「登录此槽位」——Pillow 以该槽位的身份目录为 home 起一个
`grok login` 进程,把 CLI **自己的**输出(设备码 / URL)显示出来,由用户完成。

🔴 **Pillow 全程不读 auth 文件、不解析 token、不代填。** 它只提供目录和进程。
这条和现有的「DTO 无密钥、SecretBytes 脱敏」是一致的,不要开例外。

🔴 **绝不回落。** 调研 §4.2.3 把 OpenClaw 的「二级 agent 凭据过期就读 main 的 token」
明确列为**反面教材**。槽位没有自己的凭据就显示「未登录」,**不许静默用别的槽位或
默认 profile 的会话**。缺凭据是一个要用户处理的状态,不是一个要产品掩盖的状态。

### 3.3 账号可见

`cli-detect` 的 `AuthState::Authenticated { account_label }` 已经能读出账号名,
`runtime_slots` 表**已经有 `account_label` 列**(OAuth 槽位在用)。

**动作**:登录槽位创建/登录/探活后把账号名存下来并显示在槽位上。

🔴 DTO 是闭集,加字段必须**同一个改动**里同步渲染层白名单 —— 这个坑已经踩过 3 次以上
(只加宿主字段会让 UI 静默变空,而门是绿的)。`account_label` 已在 DTO 上,
所以这次只是填值,不需要放宽白名单。**填之前先确认这一点仍然成立。**

### 3.4 从零:没有 CLI 时说人话

**动作**:检测不到 grok CLI 时,不再抛环境变量文案。给出:
1. 「本机没有找到 Grok CLI」
2. 官方安装方式
3. 「装好后点这里重新检测」
4. 手填可执行文件路径(保留现有能力)

🔴 **不要假装能对话。** 没有运行时就是没有,不要退化成 fake 适配器
—— ADR-0003 Q2 已经因为 fake 适配器在所有构建里可见修过一次。

### 3.5 Start 按钮语义分家(调研 §4.3.4)

`start_runtime_slot_impl` 对 NativeAdapter 路径只返回 `native_adapter_prepared`,
**故意不**激活对话。用户以为 Start = 「这个槽位开始干活了」。

**动作**:UI 分成两个状态点 —— 「可对话(已激活)」与「可被派单(路径已准备)」。

---

## 4. 切片顺序(每片自带验收)

1. **登录槽位的身份目录** —— 3.1。验收:两个登录槽位指向不同目录时,
   `build_slot_grok_config` 产出不同的 `honor_isolated_home`;变异:去掉隔离 → 测试变红。
2. **账号可见** —— 3.3。验收:建槽位后槽位上显示账号名;缺凭据显示「未登录」而非空白。
3. **从零文案** —— 3.4。验收:PATH 上没有 grok 时,错误里不出现环境变量名与「fake」。
4. **在槽位里登录** —— 3.2。🔴 **维护者约束:我只做到「未登录/目录不对时报错正确」为止,
   拿真账号跑通那一步由维护者本人完成**(RUNBOOK 禁止在该设备上用其账号做登录操作)。
5. **Start 语义分家** —— 3.5。

## 5. 明确不做

- ❌ 不做凭据回落(调研 §4.2.3)
- ❌ 不做产品级 YOLO / 子槽位自动批准(调研 §4.2.2)
- ❌ 不把身份搬进家目录大 JSON(调研 §4.2.1,Pillow 的 SQLite 更好)
- ❌ 不新增第三种 `RuntimeSlotGovernance`(治理仍由 auth 推导)
- ❌ 不替 Pillow 分发或自动安装 grok CLI —— 只检测、只指路
