# Pillow Agent Entry

Read and follow `AGENTS.md` in this repository.

Before substantial work, connect through Keeper Bridge:

```text
POST http://127.0.0.1:7890/api/agent-bridge/boot
{"project":"pillow","task":"<task>","agentName":"<agent>","agentRole":"controller|worker"}
```

Shared knowledge rules live at `<workspace>/VaultSystem\vault\AGENTS.md`.
The legacy resource manager at `<workspace>/VaultSystem\keeper` is a separate
read-only dependency and must not be modified from this repository.
