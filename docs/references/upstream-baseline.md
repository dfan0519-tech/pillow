# Upstream Reference Baseline

Captured: 2026-07-29

These repositories are design and implementation references. Their APIs and
current branches may change, so every direct port must record its exact source
commit.

| Project | Commit inspected | License | Adopted lessons |
| --- | --- | --- | --- |
| `anomalyco/opencode` | `7565e03536d19e850f9996c407f9bf5e932b5f7a` | MIT | provider-neutral core, package boundaries, desktop/server separation, plugin and protocol surfaces |
| `RongleCat/grok-app` | `1c3f9c8aa3a42e869f11afb3d075f1b52843aaa9` | MIT | Tauri host, React workbench, host-owned ACP session FSM, project/session UX, permissions, resource previews |
| `openai/codex` | `fe01054a28fa4bd04716d9ceadb410f2443a50ce` | Apache-2.0 | app-server protocol, thread persistence, tools, approvals, sandboxing, process hardening |
| `xai-org/grok-build` | `5da6962e4adb9c857f3def762542b52b4ec3e522` | Apache-2.0 | ACP, agent lifecycle, workspace/checkpoints, tool runtime, prompt queue, MCP and plugin boundaries |

## Reuse Policy

Three reuse levels are allowed:

1. **Conceptual reference**: architecture or interaction ideas are implemented
   independently.
2. **Adapted implementation**: a focused upstream implementation is ported and
   materially changed. The source commit, path, license, and change notice must
   be recorded in `THIRD_PARTY_NOTICES.md`.
3. **Direct dependency**: a released package or protocol crate is consumed
   through its public API and locked in the dependency manifest.

Copying a whole upstream application or hiding provenance is prohibited.

## Grok App UI Baseline

The first Keeper IDE shell deliberately follows Grok App rather than inventing
a new visual language:

- compact project and session navigation;
- one dominant conversation/workbench surface;
- quiet neutral light theme as the default;
- restrained borders, spacing, type scale, and status colors;
- visible but secondary permissions and runtime metadata;
- streaming sessions remain alive when the user changes conversations;
- artifacts, diffs, files, and previews open as contextual resources;
- infrastructure settings stay outside the primary conversation path.

This is a baseline, not permanent branding. Keeper-specific visual design begins
only after the workbench interaction passes usability acceptance.

