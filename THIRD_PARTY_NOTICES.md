# Third-Party Notices

## Grok App

Keeper IDE's workbench baseline was inspected from
[`RongleCat/grok-app`](https://github.com/RongleCat/grok-app) at commit
`1c3f9c8aa3a42e869f11afb3d075f1b52843aaa9`.

Grok App is licensed under the MIT License. Any future adapted source file
will retain the required copyright and license notice and identify its original
repository path in that file.

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
