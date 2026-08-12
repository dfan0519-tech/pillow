# Pillow 总控交接手册(2026-08-12)

工作目录:`<repo>`
分支 / HEAD:`main` / `12ce588`
✅ **工作树已清空** —— 那 40 个改动已落地,见 §2。

> 取代 `docs/handoff/2026-08-11-*.md`(那份已被清扫补丁更新过,仍可读,
> 但槽位/执行路径以本份为准)。
> 运维细节在 `.superpowers/RUNBOOK.md` 与 `.superpowers/CODEX-DISPATCH.md`(均 gitignored)。

---

## 1. 一句话状态

**外审已完成并交回报告,发现 6 个新问题(2 个 P1)。修复在途,尚未提交。**

🔴 **用户仍未跑过一轮真实任务。** 这条从 8-11 记到现在,没有变。

---

## 2. ✅ 那 40 个改动已落地(两个提交)

| 提交 | 内容 |
| --- | --- |
| `407a0dd` | 设置页信息架构重做(grok4/round45)+ 权限两轴化 + OAuth 登录链修复(grok2/round44) |
| `12ce588` | EA-01 / EA-02 修复(grok4)+ **渲染层接线**(总控补)+ 外审简报纠错 |

**证据**:门 `GATE_EXIT=0`(8 步全过)、E2E **10/10 exit 0**。

🔴 **门跑了四次才绿**,红过三次,每次都是真问题:
1. `check-secrets` —— **这份交接报告自己**泄漏了宿主用户目录路径(见 §6)
2. `cargo fmt` —— 全在 grok4 的文件里(它报告写"门禁完成",跑的其实是
   `cargo test -p ...` 三个 crate)
3. `cargo test --workspace` —— `permission_rules_wiring.rs` 跨 crate 回归,
   grok4 的分 crate 复跑看不见

🔴 **后台任务的 exit code 不是门的 exit code。** 第一次跑完,任务通知报
"exit code 0",日志里 `GATE_EXIT=1`。**永远读日志里的 `GATE_EXIT`。**

---

## 3. 外审报告 —— 本次最重要的产出

**`docs/references/external-audit-report-2026-08-12.md`**(542 行,已提交)

在我们自陈的 13 条已知缺口之外,**新发现 6 条**:

| ID | 级别 | 问题 | 后果 |
| --- | --- | --- | --- |
| **EA-01** | **P1** | Windows 大小写别名绕过项目 deny | `accept_edits`/`bypass` 下**无需新的人类决定**即可写入 deny 目录 |
| **EA-02** | **P1** | process rule subject 丢失 argv 边界 | 已保存的 allow 规则批准**语义不同**的 argv |
| **EA-03** | P2 | renderer 实际持有写审批能力 | grant 可回收利用;空 pattern 绕过前端校验植入全局规则 |
| **EA-04** | P2 | 截断 diff 可批准完整 sealed content | 用户看不到尾部却批准了全部 |
| **EA-05** | P2 | 派单 initiator 未绑定当前项目 Controller | 可伪造发起者、错误归因 |
| **EA-06** | P3 | Unicode 双向控制字符伪装审批文本 | 人眼看到的参数顺序 ≠ 实际 argv |

**外审给的发布判断(我采纳):**
> EA-01 / EA-02 关闭前,不应把"项目 deny 始终优先"当作安全承诺;
> EA-03 / EA-04 关闭前,不应把 renderer 描述为纯 view。

### ✅ EA-03 推翻的那句话已改掉(`12ce588`)

简报 §1 原文是 **"The renderer is a view. It never executes, never holds
authority."** —— **是错的**,而且是我最有把握的一句。

已就地改写,并写明"这是上一轮外审证伪的",免得下一轮再推导一遍。

### EA-03 与 EA-05 是同一类

**把只在渲染层成立的约束当成系统性质。** 正常 UI 传 `controller.slotId`,
但那是"渲染层的礼貌",原生边界上可以伪造。审计把这两条归成一类,应当一起修。

---

## 4. 你截图里的四个问题(用户 8-12 提出)

| # | 问题 | 状态 |
| --- | --- | --- |
| 1 | 两个路径字段逻辑 + 目录边界 | ✅ 已做(在未提交的 40 个改动里) |
| 2 | 登录不弹网页 | ✅ 已修(同上) |
| 3 | 设置页没有分区 | ✅ 已做(同上) |
| 4 | 权限规则逻辑奇怪 | ✅ 调研 + 两轴实现(同上) |

### 第 2 条的根因值得记住

断点**不在浏览器**,在更前面:
```
渲染层 invoke create_slot(authKind:"oauth")  → 正常
create_slot 已注册                           → 正常
宿主不接受 "oauth" → Unknown auth_kind       → ★ 断点
从未走到 open_system_browser                 → 传输层有实现,创建路径没人调
```
**OAuth 传输层和 OAuth 创建路径是两个 Worker 分别做的,各自测试全绿,
中间那一段没有人测。两半都对,接缝是空的。**

另外:用户截图里的 `Grok · 登录` 是 `GrokLogin`(复用 grok CLI 会话),
**从设计上就不弹浏览器**。两个都叫"登录"、行为完全不同 —— 命名误导已在文案修正。

### 第 4 条的调研结论(`docs/research/2026-08-12-permission-ux.md`)

🔴 **OpenCode / Grok Build / Codex 三家全部把"自动批准"与"可访问范围"分成两个轴。**
Codex 源码里 approval=`Never` 仍会 `PATCH_REJECTED_OUTSIDE_PROJECT_REASON`
拒绝写项目外 —— **宁可拒绝,也不静默放行**。

用户先要"全开时目录外也不问",后说"和主流 CLI 一样就行"。
**两条冲突,按后者执行**(用户明确)。两级模型见
`.superpowers/round44/DECISION_permission_two_tier.md`:
`有效模式 = 全局允许 ∧ 项目选择`,项目不能突破全局。

调研还有一句判断值得保留:**这是预期管理 bug,不是功能 bug。**
文案「全开:写入与命令都自动」在中文里读作无界,而执行器实际守着项目边界。

---

## 5. 在途 Worker

| 槽位 | 任务 | 状态 |
| --- | --- | --- |
| grok4 | EA-01 + EA-02(P1) | ✅ 已验收并提交(`12ce588`),验收记录见 `docs/execution/2026-08-12-ea01-ea02-acceptance.md` |
| ~~codex~~ | EA-05 | ❌ 上一次派错仓库已终止 —— 见 §7;简报仍可用 |

**简报都已写好**,在 `.superpowers/round46/`:

| 单 | 简报 | 建议槽位 | 文件集 |
| --- | --- | --- | --- |
| EA-03 | `BRIEF_CODE_ea03.md` | grok4(heavy) | write DTO / 权限规则原生边界 |
| EA-05 | `BRIEF_CODE_ea05.md` | codex | role-dispatch |
| EA-06 | `BRIEF_CODE_ea06.md` | grok2 | agent-protocol 校验 + execution 渲染 |
| EA-04 | `BRIEF_CODE_ea04.md` | 🔴 **必须等 EA-03 收口** | 与 EA-03 在 write 审批链上直接冲突 |

三份简报的文件集**互不重叠**,可以同时派。EA-04 不行。

---

## 6. 槽位与派单(**这两天踩的坑,别再踩**)

### Grok

🔴 **`~/.grok1`..`~/.grok5` 是伪 home 目录**,各自有独立的 `.grok/` 凭据。
正确路由是把 **`USERPROFILE` 与 `HOME` 指向槽位 home**:

```bash
# <槽位 home> = 维护者主目录下的 .grokN。具体值见 .superpowers/RUNBOOK.md,
# 不要写进仓库文档 —— 门里的 secret 扫描会拦下宿主用户目录路径。
USERPROFILE='<槽位 home>' HOME='<槽位 home>' \
CARGO_TARGET_DIR=<repo>/target-grok4 \
grok --prompt-file <简报> --permission-mode bypassPermissions --cwd <repo>
```

🔴 **只设 `GROK_HOME=<slot-home>` 会去找 `<slot-home>/auth.json`(空的),
新建一份平行凭据,并把已注册槽位误报成未登录。**

**当前状态**:grok2 ✅ / grok4 ✅(heavy,优先跑重活)/ grok5 ❌ 未认证 /
grok1 与 grok3 = **默认 profile,同一个账号,已耗尽(402)**。

🔴 **我一整天以为 grok1 和 grok3 是两个槽位,其实两次都跑的默认 profile。**
额度是被同一个账号连跑一天耗掉的。

### Codex

见 **`.superpowers/CODEX-DISPATCH.md`**。要点:
- 🔴 **派 Pillow 必须设 `AGENT_REPO=<repo>`** ——
  wrapper 缺省目标仓是 `denglou-ui-v2`,**不是调用它时所在的目录**。
  这条今天才补进文档;**文档漏了它,正是我派错仓的直接原因**。
- 🔴 **派完立刻看 runlog 第一行的 `repo=`**,不是 Pillow 就当场停。
- CLI 在 `F:/npm-global`,**裸 `codex` 会命中 C 盘的坏 shim**
- `CODEX_MODEL=sol` 非法,**exit 0 但零产出**
- 🔴 **全机同时只跑 1 个 `codex exec`**

---

## 7. 🔴 我今天犯的错(**代价与教训**)

1. **派 codex 派错了仓库。** wrapper `codex_run.sh` 的默认目标仓是
   `<workspace>/denglou-ui-v2`,不是调用它的当前目录。日志第一行就写着
   `[_agent_repo] 目标仓 = <workspace>/denglou-ui-v2`,**我没看就让它跑了**。
   已终止;**Pillow 未被触碰**,EA-05 报告未落盘。
   denglou 有 1403 个改动文件,但它本就在活跃分支上有在制工作 ——
   🔴 **我不能证明那些是不是我造成的,需要维护者自查。**
   **教训:用别的仓库的 wrapper 前,先确认它的目标仓怎么决定。**

2. **判"codex 要重装"是错的。** 真 CLI 在 `F:/npm-global`,
   只是 C 盘一个坏 shim 排在 PATH 更前面。
   我查了 Store 应用、查了 npm 目录为空,**就是没查 PATH 顺序**。

3. **从单点信号推全局。** 一个槽位 402,我说成"再派 grok Worker 都会失败"。

4. **红着门推了一次。** 把 commit 链在门检查后面却没判它的退出码 ——
   跑了门又无视结论,比不跑更糟,因为日志看起来像验证过了。已补 `f91c049`。

5. **把运维约束读成架构禁令。** 用户说"别在我设备上用我账号登录",
   我做成了"代码里禁止 Claude/Codex OAuth",**正好禁掉了用户要保留的功能**。

6. **验收时漏了接缝。** OAuth 传输层与创建路径两个 Worker 各自绿,
   **中间没人测**(§4)。**两半都绿不等于链路通。**

7. **杀进程前先查父进程。** 18 个 `msedgewebview2` 看着像 E2E 残留,
   实际父进程是 Widgets / SearchHost / a local proxy —— **按名字杀会干掉用户的应用**。

---

## 8. 磁盘

本轮清理释放约 **122 GB**(F: 现 293 GB 空闲):
`target/debug` 68 GB、12 个历史 worker target、`artifacts/` 108 个旧日志。

🔴 **保留了 `target/release`(含 `pillow.exe`,用户快捷方式指向它)。**
🔴 `target/debug` 已删,**下次跑门会触发一次全量 debug 构建**(十几分钟)。

槽位伪 home 里的 `sessions` 共 **11.85 GB** —— 那是历次 Worker 的对话历史,
**是用户数据不是缓存,未删,需用户决定**。

---

## 9. 下一步(按顺序)

1. ~~跑完 E2E,提交那 40 个改动~~ ✅ `407a0dd` + `12ce588`
2. ~~收 grok4 的 EA-01/02,变异验证~~ ✅ 见 §2、§5
3. ~~修正外审简报那句 "renderer is a view"~~ ✅ `12ce588`
4. **派 EA-03(grok4)/ EA-05(codex)/ EA-06(grok2)** —— 简报已就绪
5. EA-04 等 EA-03 收口后再派
6. 🔴 **用户跑一轮真实任务** —— 全系统仍是零实盘证据,这条从 8-11 记到现在没变
7. **打包**(用户明确:最后一轮)

---

## 10. 环境

- 门:`node scripts/run-worker-gate.mjs`(唯一合规入口)
- E2E:`corepack pnpm desktop:test:e2e`,约 25 分钟,**总控独占**
- 🔴 **日志写 `.superpowers/logs/`,不要写 `/tmp`** ——
  另一个 Claude Code 进程的启动清理会删掉 `/tmp` 下的文件,
  今天丢过一次 E2E 日志
- 台账体检:
  `knowledge-indexer audit --db .superpowers/ledger/pillow.db --project pillow --repo . --doc <handoff> --now <RFC3339> --stale-after-hours 24`
  (退出码 0=全过 / 3=有 FAIL / 1=工具出错)
- 🔴 **派单后要记得 `task complete` + `verdict record --commit <id>`** ——
  今天漏了 9 条,是体检自己抓出来的
