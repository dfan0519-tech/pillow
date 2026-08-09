# Handoff renderability matrix (K1b)

Date: 2026-08-09  
Reference handoff (read-only, **not modified**):
`docs/handoff/2026-08-08-pillow-controller-handoff.md`  
Render entrypoint: `keeper_knowledge_indexer::render_handoff_view`

## Judgment classes

| Class | Meaning |
| --- | --- |
| **已可渲染** | Ledger/registry has the data shape; render produces equivalent content when rows exist |
| **可渲染但数据缺失** | Structure supports it, but nobody has registered yet → which action must write |
| **不可渲染** | Human judgment / narrative / external ops cookbook — do not fabricate |

## Section-by-section

| § | Handwritten topic | Class | Evidence / registration path |
| --- | --- | --- | --- |
| Header | Workdir, branch, HEAD, remote | **不可渲染** | Git snapshot not in knowledge tables; optional future scan metadata only |
| Supersedes blurb | “本文件取代 A 与 B” | **已可渲染** | Registry `supersedes` links (§4.3 of composite); path inventory |
| §1 一句话状态 | “C4 checkpointed… 在途 Worker:0… next step” | **可渲染但数据缺失** (ops half) + **不可渲染** (prose) | In-flight count + recent verdicts when ledger filled (`task dispatch` / `verdict record`). Free-form “下一步” narrative is judgment |
| §2 产品终点 | User-confirmed product intent + Stage lines | **不可渲染** | Product intent is human confirmation, not a document-role fact |
| §2 两条红线 | Stage-2 red lines awaiting user yes | **不可渲染** | Open decisions / verbal confirmation |
| §3 四步计划进度 | Steps 1–4 status table | **可渲染但数据缺失** | No plan-step status table yet; would need explicit plan rows |
| §4 已完成关键工作 | Narrative of C4, path converge, gates, flakes, S0a/b… | **可渲染但数据缺失** (verdict shells) + **不可渲染** (deep diagnosis) | `verdict.summary` / `residuals` hold one-liners; multi-paragraph root-cause lives in report bodies only |
| §4.x 各子节报告路径 | Pointers to execution reports | **已可渲染** | Registry inventory by role/path (document status) |
| §5 Round 8/9 验收 | UI/harness acceptance narrative | **可渲染但数据缺失** | Same as verdicts; currently empty unless controller records |
| §6 下一步 | Checklist of next actions | **不可渲染** | Ranking / prioritization is controller judgment |
| §7 未解决风险 | Risk list with residual state | **可渲染但数据缺失** (residuals field) + **不可渲染** (risk essay) | `verdict.residuals` can hold one-liners; pitfall essays cannot |
| §8 操作手册 | Grok/Codex claim-release, env wrappers, iron rules | **不可渲染** | Operational runbook; must stay human-maintained (or separate living doc outside this ticket) |
| §9 我踩过的坑 | Experiential “do not repeat” log | **不可渲染** | Experiential narrative |

## Counts

| Class | Sections / rows |
| --- | ---: |
| **已可渲染** | **2** (supersession blurb; report-path inventory) |
| **可渲染但数据缺失** | **5** (§1 ops half; §3 plan table; §4/§5 verdict shells; §7 residual one-liners) |
| **不可渲染** | **8** (header git; §1 prose; §2 product+red lines; §4 deep narrative; §6 next steps; §7 risk essay; §8 runbook; §9 pitfalls) |

Some rows are dual-class (structure vs prose). Dual rows count once in the class that **blocks** full replacement; the table above lists the blocking class second where both apply. Pure dual tallies for planning:

- Structural coverage exists for: in-flight, pending acceptance, verdicts, document inventory, supersession, audit buckets, unknown ratio, archive default-hide.
- Still requires human: product intent, plan narrative, cookbooks, ranked next steps, experiential pitfalls, git identity line.

## What the composite actually emits (when data present)

| Composite § | Source |
| --- | --- |
| Honesty: `unknown` ratio | Audit / document statuses |
| 1. 当前在途 | `tasks` where `status=dispatched` (title, slot label, brief path) |
| 2. 待验收 (prominent) | `completed` tasks with no matching verdict — **listed item-by-item** |
| 3. 最近验收结论 | `verdicts` with **summary** + **residuals** |
| 4. 文档状态 | K0c inventory / lifecycle / supersession / audit |
| 5. 本渲染仍替不掉什么 | Fixed honest-gap table (always present) |

Archived documents: **default excluded** from the body (K1a / ADR-0004 §5).

## Implications for the next controller

1. Wire `task dispatch` / `task complete` / `verdict record` into the real dispatch/acceptance path (K0e CLI already exists) — otherwise §1–§3 stay empty while work continues off-ledger.
2. Do **not** delete the handwritten runbook (§8) or pitfall log (§9) until a separate durable home exists; the render correctly refuses to fake them.
3. Plan-step rows and “active pointer” marks remain optional follow-ups (called out in K0c residual list).
