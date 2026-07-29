# Third-Party Notices

## Grok App

Repository: [`RongleCat/grok-app`](https://github.com/RongleCat/grok-app)

Fixed commit: `cbbbdf17bb8db1588b6707f3f42f6b00397e27f9`

License source: `LICENSE` at the fixed commit above.

### Shell prototype adaptations

The following Keeper IDE files adapt layout or implementation patterns from
the corresponding Grok App paths at the inspected commit above:

| Keeper IDE path | Upstream path |
| --- | --- |
| `apps/desktop/src/App.tsx` | `src/App.tsx` |
| `apps/desktop/src/components/WindowControls.tsx` | `src/components/WindowControls.tsx` |
| `apps/desktop/src/features/projects/ProjectSidebar.tsx` | `src/App.tsx` project tree, `src/styles/app.css` (`.sidebar`, `.sidebar-nav`, `.nav-new`) |
| `apps/desktop/src/features/sessions/ConversationSidebar.tsx` | `src/App.tsx` session rail |
| `apps/desktop/src/features/workbench/Workbench.tsx` | `src/App.tsx` main stage, `src/components/lobe-chat/ConversationThread.tsx` |
| `apps/desktop/src/features/workbench/Composer.tsx` | `src/components/ComposerEditor.tsx`, composer styles in `src/styles/app.css` |
| `apps/desktop/src/features/workbench/ResourcePane.tsx` | `src/components/ResourceViewer.tsx`, `.aside` rules in `src/styles/app.css` |
| `apps/desktop/src/styles/tokens.css` | `src/styles/tokens.css` (light theme tokens) |
| `apps/desktop/src/styles/workbench.css` | `src/styles/app.css`, `src/App.tsx` |
| `apps/desktop/src-tauri/src/project_commands.rs` (`pick_directory`) | `src-tauri/src/commands.rs` (`pick_directory`, `rfd::FileDialog`) |
| `apps/desktop/src-tauri/src/path_scope.rs` | `src-tauri/src/path_scope.rs` |
| `apps/desktop/src-tauri/src/directory_ops.rs` | folder naming helpers used with `pick_directory` |
| `apps/desktop/src-tauri/src/main.rs` (`windows_subsystem`) | `src-tauri/src/main.rs` |

These adaptations retain general desktop layout, bounded navigation,
conversation rhythm, custom window controls, native folder picking, path
allowlisting, and neutral light tokens. They exclude Grok-specific account,
quota, wallpaper, voice, automation, model-menu, SuperGrok branding, and
relay behavior.

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
