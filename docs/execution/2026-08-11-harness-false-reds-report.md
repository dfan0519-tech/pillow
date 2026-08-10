# Harness false-reds fix report

**Date:** 2026-08-11  
**Worker:** grok1 (write)  
**Scope:** `scripts/**` + this report only  
**Signal:** `HARNESS-FALSE-REDS-COMPLETE`

## Summary

Fixed harness-side failures that turned green product runs red:

1. **T1** — JSON control characters in captured process output (PowerShell
   `ConvertTo-Json` / stream embed) no longer blow up `JSON.parse` or pollute
   record lines. Sanitize **at capture**, not by swallowing parse errors.
2. **T2** — Same pattern audited across wave/lib acceptance scripts; shared
   helper applied wherever process JSON was parsed raw.
3. **T3** — Renderer console/pageerror diagnostics ported from C4 process
   acceptance to every other `connectOverCDP` orchestrator.
4. **T4** — No `cmd | tail` / `cmd | head` exit-code traps inside `scripts/`.

Criteria were **not** relaxed: real structural JSON errors still throw; real
UI/product failures still fail.

---

## T1 — Control-character false red (before / after evidence)

### Root cause

Wave C2 (and siblings) parse PowerShell process snapshots with `JSON.parse` on
stdout. When any process command line contains raw `U+0000`–`U+001F`, producers
such as `ConvertTo-Json` can emit **unescaped** controls inside string
literals. Node then throws:

```text
Bad control character in string literal in JSON at position …
```

That becomes `KEEPER_WAVE_C2_RESOURCE_ACCEPTANCE { "status":"failed", "error":"Bad control character…" }` — a **harness** failure, not a product defect.

### Fix

Shared module: `scripts/lib/json-text-sanitize.mjs`

| API | Role |
|-----|------|
| `escapeRawControlsInJsonText` | Escape raw controls **inside** JSON strings to `\u00XX` before parse |
| `parseJsonFromCapturedText` | Always sanitize-then-parse; structural errors still throw |
| `sanitizeControlCharsVisible` | Replace controls with `<0xNN>` (no silent delete) |
| `formatCapturedStream` | Embed stdout/stderr with `originalLength` + `controlCharsReplaced` metadata |

Wired into:

- `scripts/lib/desktop-process.mjs` (`parseJsonArray`, `runChecked` error text)
- `scripts/wave-c2-resource-acceptance.mjs`
- `scripts/wave-c3-governed-write-acceptance.mjs`
- `scripts/wave-c4-git-changes-acceptance.mjs`
- `scripts/wave-c4-governed-process-acceptance.mjs`

`redactText` in those wave orchestrators also runs visible control sanitization
after path/uuid redaction so failed records stay valid JSON without hiding
odd bytes.

### Proof (temporary one-shot script; deleted after run)

Constructed capture equivalent to a bad PowerShell row:

```text
{"commandLine":"C:\\app.exe --flag<ETX>payload<NUL>end","pid":42,"name":"app.exe"}
```

**Before** (`JSON.parse` on raw capture):

```text
threw: Bad control character in string literal in JSON at position 34 (line 1 column 35)
```

**After** (`parseJsonFromCapturedText`):

```text
{"ok":true,"originalLength":74,"controlCharsEscaped":2,"commandLine":"C:\\app.exe --flag\u0003payload\u0000end","pid":42}
```

**Record embed** (`formatCapturedStream` → `JSON.stringify` → re-parse):

```text
{"recordParseOk":true,"errorHasPlaceholders":true,"metaPresent":true}
PROOF_OK
```

Self-test (durable):

```text
JSON_TEXT_SANITIZE_SELF_TEST {"status":"passed","controlCharsEscaped":2,"visibleReplaced":1}
```

---

## T2 — Sibling risk survey

| File | Pattern | Conclusion |
|------|---------|------------|
| `wave-c2-resource-acceptance.mjs` | `JSON.parse` on PS snapshot; raw stdout/stderr in errors | **Fixed** |
| `wave-c3-governed-write-acceptance.mjs` | same | **Fixed** |
| `wave-c4-git-changes-acceptance.mjs` | same | **Fixed** |
| `wave-c4-governed-process-acceptance.mjs` | same + existing renderer diag | **Fixed** (parse + streams) |
| `lib/desktop-process.mjs` | `parseJsonArray` on PS process list | **Fixed** |
| `lib/c2-resource-ui-acceptance.mjs` | only `JSON.stringify` in assert messages | No process capture → JSON.parse risk |
| `lib/c2-activity-ui-acceptance.mjs` | same | Clean |
| `lib/c3-governed-write-acceptance.mjs` | stringify of body snippets in asserts | Clean for parse; stringify escapes controls |
| `lib/c4-*-acceptance.mjs` | same | Clean for parse |
| `wave-c1-visual-acceptance.mjs` | report via `JSON.stringify`; no PS process JSON | Clean |
| `conversation-recovery-desktop.mjs` | summary via `JSON.stringify` | Clean |
| `native-acceptance-desktop.mjs` / `smoke-desktop.mjs` | summary stringify | Clean |
| `native-acceptance-grok-acp.mjs` | `JSON.parse` of adapter evidence file | **Residual:** controlled fixture/adapter output; not PS process lists. If adapter ever writes raw controls, same class of bug could appear — not observed in the C2 false-red. |
| `probe-mcp-tool-exposure.mjs` | NDJSON line parse with skip-on-fail | Framing noise is skipped by design (not harness record stringify) |
| `check-acp-protocol-drift.mjs` / `check-workspace.mjs` | parse repo-owned JSON files | Clean |
| `lib/desktop-process.mjs` `waitForJsonFile` | parse readiness JSON from app | **Residual:** app should emit valid JSON via serde; not sanitized (would mask malformed readiness — prefer hard fail) |

No silent try/catch was added around parse to invent a pass.

---

## T3 — Renderer diagnostics port

Reference shape from `wave-c4-governed-process-acceptance.mjs`:
`attachRendererDiagnostics` / `rendererDiagnostics` / attach inside
`connectWebViewPage` / include on phase failure.

| Script | CDP? | Action |
|--------|------|--------|
| `wave-c4-governed-process-acceptance.mjs` | yes | Already had collection; control-char sanitize on push; **fixed** launch-phase reference to undefined `connectedPage` (would throw `ReferenceError` and mask the real launch error) |
| `wave-c2-resource-acceptance.mjs` | yes | **Ported** |
| `wave-c3-governed-write-acceptance.mjs` | yes | **Ported** |
| `wave-c4-git-changes-acceptance.mjs` | yes | **Ported** |
| `wave-c1-visual-acceptance.mjs` | **no** (`connectOverCDP` absent; Chromium screenshot harness) | N/A |
| `conversation-recovery-desktop.mjs` / smoke / native-acceptance | **no** CDP | N/A |

Path redaction remains via existing `redactText` when folding diagnostics into
failed records (`<path>` / `<uuid>`), matching the C4 baseline contract.

---

## T4 — Exit-code traps

### Inside `scripts/`

Search for `| tail`, `| head`, and shell pipelines that discard status:

- **No matches** of `cmd | tail` / `cmd | head` in `scripts/**`.
- PowerShell one-liners use `|` only **inside** a single `powershell.exe -Command`
  (CIM → `ConvertTo-Json`); the spawn exit code is still that of PowerShell.

### Outside `scripts/` (listed only)

- `docs/handoff/2026-08-11-pillow-controller-handoff.md` documents a controller
  habit of piping E2E to `tail`, which ate the exit code. Not product code;
  not modified here.

---

## Verification commands (exit codes)

No cargo / gate / desktop E2E (other worker compiling; E2E controller-owned).

| Command | Exit |
|---------|------|
| `node --check scripts/lib/json-text-sanitize.mjs` | 0 |
| `node --check scripts/lib/desktop-process.mjs` | 0 |
| `node --check scripts/wave-c2-resource-acceptance.mjs` | 0 |
| `node --check scripts/wave-c3-governed-write-acceptance.mjs` | 0 |
| `node --check scripts/wave-c4-git-changes-acceptance.mjs` | 0 |
| `node --check scripts/wave-c4-governed-process-acceptance.mjs` | 0 |
| `node scripts/lib/json-text-sanitize.mjs --self-test` | 0 |
| `node scripts/_tmp-control-char-proof.mjs` (then deleted) | 0 |
| `node scripts/check-secrets.mjs` | 0 |

---

## Write-boundary audit

**This worker’s intentional edits:**

- `scripts/lib/json-text-sanitize.mjs` (new)
- `scripts/lib/desktop-process.mjs`
- `scripts/wave-c2-resource-acceptance.mjs`
- `scripts/wave-c3-governed-write-acceptance.mjs`
- `scripts/wave-c4-git-changes-acceptance.mjs`
- `scripts/wave-c4-governed-process-acceptance.mjs`
- `docs/execution/2026-08-11-harness-false-reds-report.md` (this file)

**Not touched by this worker:** `crates/**`, `apps/**` (another worker owns
`crates/adapter-grok-acp/**`). Temporary proof script removed after run.

No git write operations.

---

## Residual risks

1. **`waitForJsonFile` readiness parse** still uses raw `JSON.parse`. Intentional:
   malformed readiness is a real launch failure; sanitizing could hide it.
2. **Adapter / MCP NDJSON paths** outside wave CDP orchestrators were not
   rewired; they are not the observed C2 false-red path.
3. **Load-induced** false reds (`can't find crate for std`, etc.) are out of
   scope for this ticket (class B).
4. **Visible placeholders expand length** (`\x03` → `<0x03>`). Metadata
   `originalLength` / `controlCharsReplaced` records the pre-sanitize size so
   post-hoc review can tell replacement happened.

---

## HARNESS-FALSE-REDS-COMPLETE
