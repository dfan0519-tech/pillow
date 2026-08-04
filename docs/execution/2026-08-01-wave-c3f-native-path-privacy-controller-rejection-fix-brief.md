# Wave C3-F Controller Rejection Fix — Native Path Privacy

Owner: Grok renderer/harness worker; controller owns review and acceptance

Worker session: `<keeper-session>`

Runtime allocation: `grok1` / `<keeper-lease>`

Status: ACTIVE. The C3-F resume delivery is rejected on this boundary.

## Rejection

The worker changed the C3-F privacy gate so absolute host-path tokens were no
longer checked in full body HTML, citing the existing
`title={project.rootPath}` tooltip. The active C3-F visual gate forbids native
paths in screenshots or accessibility text. A native project root in a
`title` attribute is therefore a product privacy defect, not an allowed test
exception.

## Required Contract

1. Remove the raw native `project.rootPath` from all sidebar DOM text and
   attributes. Preserve project selection and the visible project name.
2. A safe name-only tooltip is allowed; do not derive or display a parent path,
   drive letter, UNC prefix or file URL.
3. Restore the C3-F privacy assertion so full body markup cannot contain the
   temporary projects' native roots or generic host-path prefixes.
4. Keep portable project-resource paths inside the resource/diff surfaces;
   this task must not suppress legitimate relative paths.
5. Do not change project DTOs, native commands, persistence or the visual
   layout.

Use TDD: first prove a project whose root contains a sentinel native path leaks
through rendered DOM attributes, then make the smallest renderer change and
restore the strict harness assertion.

## Write Whitelist

- `apps/desktop/src/features/projects/AppSidebar.tsx`
- `apps/desktop/tests/project-workbench.test.tsx`
- `apps/desktop/tests/visual-shell-contract.test.tsx` only if needed for the
  existing sidebar contract
- `scripts/lib/c3-governed-write-acceptance.mjs`
- `docs/execution/2026-08-01-wave-c3f-native-path-privacy-fix-report.md`

Do not edit runtime, native commands, storage, policy, executor, write approval
components, other C3-F harness files or fake ACP fixtures.

## Gates

- focused sidebar/privacy Vitest tests;
- Node syntax check for the C3 helper;
- desktop typecheck;
- desktop production build;
- whitelist `git diff --check`;
- HEAD unchanged and staging empty.

## Completion

Write the report with RED/GREEN evidence and `COMPLETION: DONE` or
`COMPLETION: BLOCKED`, send a completion message to controller session
`<keeper-session>`, then exit. Do not reset, restore, stage,
commit, package, publish or write to the frozen old Keeper tree.
