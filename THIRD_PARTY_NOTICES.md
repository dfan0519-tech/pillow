# Third-Party Notices

## Grok App

Repository: [`RongleCat/grok-app`](https://github.com/RongleCat/grok-app)

Fixed commit: `cbbbdf17bb8db1588b6707f3f42f6b00397e27f9`

License source: `LICENSE` at the fixed commit above.

### Shell prototype adaptations

The following Pillow files adapt **specific** layout, control grammar, or
host patterns from Grok App at the inspected commit. Claims are limited to the
structures listed; they do not assert a full component-for-component port.

| Pillow path | Upstream source (exact) | What was reused |
| --- | --- | --- |
| `apps/desktop/src/App.tsx` | `src/App.tsx` | Always-on shell regions (sidebar + main stage); no launcher full-screen |
| `apps/desktop/src/components/WindowControls.tsx` | `src/components/WindowControls.tsx` | Custom titlebar minimize / maximize / close control grammar |
| `apps/desktop/src/components/icons.tsx` | `src/components/icons.tsx` | `g-icon` wrap pattern and stroke-icon call-site shape (Tabler not vendored) |
| `apps/desktop/src/features/projects/AppSidebar.tsx` | `src/App.tsx` sidebar tree; `src/styles/app.css` `.sidebar`, `.sidebar-brand-row`, `.sidebar-nav`, `.nav-new`, `.nav-item`, `.tree-l1`, `.project-row`, `.session-list` | Continuous left rail: brand row, ghost nav, collapsible projects, nested sessions, restrained footer |
| `apps/desktop/src/features/workbench/Workbench.tsx` | `src/App.tsx` main stage chrome | Quiet center stage + top-right resource icon toggle |
| `apps/desktop/src/features/workbench/Composer.tsx` | `src/styles/app.css` `.composer`, `.composer-wrap--float`, `.icon-btn`, `.icon-btn--primary`; layout notes from `src/components/ComposerEditor.tsx` | Floating ~18px card, soft shadow, circular send, project context chips |
| `apps/desktop/src/features/workbench/ResourcePane.tsx` | `src/components/ResourceViewer.tsx` shell chrome only | Aside open/close chrome; content remains a Keeper stub |
| `apps/desktop/src/styles/tokens.css` | *(structure only; colors no longer from upstream)* | Token **names**, dark single-theme shape, and control-token categories (bg / text / accent / radius / shadow aliases) follow the Grok App tokens layout pattern. **Color values are Pillow-original** (Concept 08 «Mirage Loop»: BLACK INK `#0E0E10`, GRAPHITE `#3E3E46`, LILAC HAZE `#DCCBFF`, OFF-WHITE `#FAF9F7`, SMOKE GRAY `#C7C7CE`) and are **not** derived from Grok App light or dark palettes (`#3d5fd9` / `#8aa4ff` are not claimed). Glass surface tokens (`--glass-*`) are Pillow-original. |
| `apps/desktop/src/styles/workbench.css` | `src/styles/app.css` (selected rules above) | Ported control grammar for sidebar, chrome-btn, composer, icon-btn; surface fills may use Pillow glass tokens |
| `apps/desktop/src-tauri/src/project_commands.rs` (`pick_directory`) | `src-tauri/src/commands.rs` (`pick_directory` + `rfd::FileDialog`) | Native folder picker and grant after pick |
| `apps/desktop/src-tauri/src/path_scope.rs` | `src-tauri/src/path_scope.rs` | Absolute path allowlist grant / `is_allowed` |
| `apps/desktop/src-tauri/src/main.rs` (`windows_subsystem`) | `src-tauri/src/main.rs` | Hide extra Windows console in release |

**Not claimed as ports:** account/quota menus, model selectors, SuperGrok branding,
wallpaper, voice, automation, relay surfaces, ResourceViewer file previews,
ComposerEditor editor engine, or a dual project+conversation column layout.

Keeper product copy, SQLite project authority, open/register/create flows, and
visual-contract tests are original to Pillow.

### MIT License

MIT License

Copyright (c) 2026 RongleCat

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

---

## OpenCode

Repository: [`sst/opencode`](https://github.com/sst/opencode)

Fixed commit: `d4704347465c1ee63d0c213ed00e648e7f0231c5`

License source: `LICENSE` at the fixed commit above (MIT).

### Credential-model adaptations

Pillow's OAuth slot auth follows the **shape** of OpenCode's credential model.
No OpenCode source file is vendored; the claims below are limited to the
structures named.

| Pillow path | Upstream source (exact) | What was reused |
| --- | --- | --- |
| `crates/storage-sqlite/src/runtime_slot.rs` (`SlotAuth::Oauth`) | `packages/opencode/src/auth/index.ts` (`Oauth` / `Api` schema classes) | Credential record shape: refresh + access + expiry + optional non-secret account label, discriminated by auth type and keyed per provider |
| `crates/runtime-slots/src/lib.rs` (token injection at prepare) | `packages/opencode/src/auth/index.ts` (`OAUTH_DUMMY_KEY`) | The pattern of leaving the static key slot as a placeholder and injecting a live access token at launch time, so an OAuth provider can drive a client that otherwise expects a fixed API key |

**Not claimed as ports:** OpenCode's Effect-based service layer, its provider
plugins (xAI, GitHub Copilot, OpenAI Codex, Snowflake Cortex), its OAuth
callback page, its `models.dev` provider catalog, or any transport code.

Pillow deliberately does **not** implement consumer-subscription OAuth
(ChatGPT Plus / Claude Pro). OpenCode reaches those through vendor-specific
endpoints with rewritten authorization headers and restricted model
allowlists — a shape Pillow's grok-shell `base_url` + `env_key` path cannot
express, so it is left unsupported rather than half-supported.

### MIT License

MIT License

Copyright (c) 2025 opencode

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
