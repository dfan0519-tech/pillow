# Wave C3 Codex2 MSVC VCTIP process-boundary development report

Status: DONE_WITH_CONCERNS

Date: 2026-08-02

## Scope and contract

Only the brief allowlist was changed:

- `scripts/lib/desktop-process.mjs`
- `scripts/lib/desktop-process-safety.test.mjs`
- `apps/desktop/tests/e2e/c3-governed-write.spec.ts`
- `docs/execution/2026-08-02-wave-c3-codex2-vctip-process-boundary-development-report.md`

`ProcessSupervisor` and `runSupervisedCommand` now accept the default-off
`allowMsvcVctipTelemetry` option. The option is fail-closed: only the boolean
value `true` enables it. It can exempt only a non-root, still-observed identity
after the root is `confirmed-dead` or `identity-replaced`, and only when both
of these normalized fields match:

- name: `vctip.exe`
- executable path: a local-drive `Program Files` or `Program Files (x86)`
  root, then `Microsoft Visual Studio`, a four-digit release/year, exactly one
  edition directory, and
  `VC\\Tools\\MSVC\\<version>\\bin\\HostX64\\x64\\vctip.exe`

No process is terminated by this allowance. Incomplete identities, wrong-path
lookalikes, root identities, PID mismatch/reuse, Pillow/fake ACP/WebView
residuals, port listeners, and all default callers remain fail-closed.

The C3 E2E wrapper explicitly opts in and restores
`enforceDesktopClean: true`; the nested orchestrator remains responsible for
its own managed desktop child lifecycle and unchanged scenario behavior.

## RED/GREEN evidence

Initial RED: `node --test scripts/lib/desktop-process-safety.test.mjs` exited
1 with 33/35 passing. The exact opt-in VCTIP case remained an
observation-only residual, and the C3 wrapper still lacked the two explicit
settings.

Second RED: the strict option test exited 1 with 35/36 passing because the
truthy string `"true"` incorrectly enabled the exception.

GREEN: the focused safety suite exits 0 with 36/36 passing, including default
rejection, exact opt-in acceptance after root exit, strict boolean opt-in,
wrong-path rejection, root rejection, and retained Pillow/fake ACP/WebView
residual rejection.

## Controller-review amendment

Controller review identified that the first path matcher also accepted a
lookalike rooted at `C:\\tmp\\Microsoft Visual Studio\\...`. A new regression
first reproduced that defect: the focused suite exited 1 with 36/37 passing
because opt-in incorrectly accepted the lookalike. The matcher now requires
the explicit Program Files root, a four-digit VS release/year, and one edition
segment before the fixed MSVC tail. The observed
`C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\BuildTools\\...`
identity remains accepted, while the lookalike is rejected. Final GREEN is
37/37 passing.

## Verification

- `node --test scripts/lib/desktop-process-safety.test.mjs` — exit 0; 37
  passed, 0 failed.
- `node --check scripts/lib/desktop-process.mjs` — exit 0.

No real desktop E2E was run; that remains the controller's acceptance gate.

## Remaining risk

The exact production VCTIP identity was modeled with the observed Visual
Studio BuildTools path, but it was not re-exercised against a live C3 desktop
run in this worker task. The controller should perform that controlled
acceptance and verify its outer baseline/port cleanup record. No staging,
commit, reset, restore, or clean operation was performed, and this worker
launched no desktop process.
