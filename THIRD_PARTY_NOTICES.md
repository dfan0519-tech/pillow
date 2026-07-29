# Third-Party Notices

## Grok App

Repository: [`RongleCat/grok-app`](https://github.com/RongleCat/grok-app)

Fixed commit: `1c3f9c8aa3a42e869f11afb3d075f1b52843aaa9`

License source: `LICENSE` at the fixed commit above.

### Task 4 adaptations

The following Keeper IDE files adapt layout or implementation patterns from
the corresponding Grok App paths at the inspected commit above:

- `apps/desktop/src/App.tsx` <- `src/App.tsx`
- `apps/desktop/src/components/WindowControls.tsx` <- `src/components/WindowControls.tsx`
- `apps/desktop/src/features/projects/ProjectSidebar.tsx` <- `src/components/VirtualList.tsx`
- `apps/desktop/src/features/workbench/Workbench.tsx` <- `src/components/lobe-chat/ConversationThread.tsx`
- `apps/desktop/src/styles/tokens.css` <- `src/styles/tokens.css`
- `apps/desktop/src/styles/workbench.css` <- `src/styles/app.css`, `src/App.tsx`

These adaptations retain only general desktop layout, bounded navigation,
conversation rhythm, custom window controls, and neutral token patterns. They
exclude Grok-specific account, quota, wallpaper, voice, automation, model-menu,
and relay behavior.

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
