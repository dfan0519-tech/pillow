> **已接入共享知识仓库 <workspace>\VaultSystem\vault\**
> 项目：pillow
> 开始任何任务前，先执行 QUERY 工作流：读 `<workspace>\VaultSystem\vault\hot.md` (近期上下文) -> `<workspace>\VaultSystem\vault\index.md` (知识索引) -> 按需检索 wiki 页面和 skills。
> 完成非平凡任务后，执行 INGEST 工作流回填经验到 `<workspace>\VaultSystem\vault\wiki\projects\`。
> 完整规则见 `<workspace>\VaultSystem\vault\AGENTS.md`。
> Keeper Bridge boot: POST http://127.0.0.1:7890/api/agent-bridge/boot with {"project":"pillow","task":"<task>","agentName":"<agent>","agentRole":"controller"}.

---

# Pillow Agent Rules

This repository is the clean successor to the legacy Vault Keeper.

## Product Order

1. Industrial local AI IDE.
2. Governed Obsidian knowledge integration.
3. One-person-company project operations.

Do not pull later-stage concerns into an earlier stage without an accepted
design change.

## Repository Boundary

- Treat `<workspace>\VaultSystem\keeper` as read-only reference material.
- Do not copy legacy files wholesale.
- Port behavior through typed contracts and focused tests.
- Do not modify shared vault rules, keys, credentials, or project metadata.
- Never commit credentials, account tokens, machine-specific paths, or runtime
  session data.

## Engineering Rules

- Domain state belongs in SQLite, not ad hoc JSON files.
- The desktop shell must not own agent-domain logic.
- Agent runtimes connect through explicit adapters.
- Approval, audit, and secret boundaries are enforced below the UI.
- Every durable object is project-scoped.
- UI work follows the accepted Grok App interaction baseline until a Pillow
  design system supersedes it.
- Upstream code reuse must retain required license and attribution notices.
- New modules require focused tests and must remain independently replaceable.

## Grok CLI Slot Isolation

- Claim Grok capacity through Keeper before every worker launch and release the
  exact lease when the worker finishes or fails to start.
- `grok1` uses the default profile at `%USERPROFILE%\.grok`. Registered
  slots `grok2`, `grok3`, and `grok4` must use their Keeper wrapper semantics;
  their effective Grok profile is `<slot-home>\.grok`.
- Never launch slots 2-4 by setting only `GROK_HOME=<slot-home>`. That resolves
  `<slot-home>\auth.json`, creates a parallel credential store, and can falsely
  report the registered Keeper slot as signed out.
- Treat an outer `<slot-home>\auth.json` for slots 2-4 as routing contamination,
  not as the registered slot identity. Do not create, copy, or reuse it.
- Verify slot routing through Keeper runtime metadata and the resolved config
  source before interpreting login, quota, or worker results. Never print raw
  authentication files or token-bearing fields.

## Required Gates

- Typecheck, lint, unit tests, integration tests, and desktop smoke must pass.
- Agent adapters require protocol fixture tests.
- Write-capable tools require explicit approval-path tests.
- UI changes require desktop screenshots at supported Windows sizes.
- Performance budgets are release gates, not optional diagnostics.
