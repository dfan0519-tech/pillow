import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = fileURLToPath(new URL("../../../../", import.meta.url));
// @ts-expect-error Repository-local supervisor has no declaration file.
const { runSupervisedCommand } = await import("../../../../scripts/lib/desktop-process.mjs");

/**
 * The budget guards against a hang, not against slowness: what this spec checks
 * is geometry and skin regressions, and none of its assertions get faster or
 * slower with the clock.
 *
 * It used to allow 150s while the harness routinely took 126s (2026-08-12) and
 * 135s (2026-08-13) — about 10% of headroom. On the second full-suite run of
 * 2026-08-13 it took 154s as the last spec of a ~30 minute suite and was killed,
 * which reads as a product failure and costs another suite to disprove. Running
 * it alone immediately passed.
 *
 * Keep the observed numbers above current when you touch this. A run that
 * approaches the new budget is a real slowdown worth investigating, and there is
 * no way to tell without knowing what normal looked like.
 */
test("Wave C1 visual acceptance captures every desktop state without geometry or interaction regressions", {
  timeout: 360_000,
}, async () => {
  const result = await runSupervisedCommand(
    process.execPath,
    ["scripts/wave-c1-visual-acceptance.mjs"],
    {
      cwd: repositoryRoot,
      // Detached MSVC vctip.exe telemetry outlives the cleanup grace after a
      // debug build. Identity-gated, and only honoured once the root is dead.
      allowMsvcVctipTelemetry: true,
      enforceDesktopClean: false,
      env: process.env,
      timeoutMs: 300_000,
    },
  );
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;

  // Timeout first. A killed process reports a null exit code, so asserting the
  // status first turns "the harness ran out of time" into `null !== 0`, which
  // says nothing about what happened.
  assert.equal(result.timedOut, false, `visual acceptance timed out\n${output}`);
  assert.equal(result.status, 0, output);
  assert.match(output, /KEEPER_WAVE_C1_VISUAL_ACCEPTANCE/u);
  assert.match(output, /"status":"passed"/u);
  assert.match(output, /"captures":\[/u);
  assert.match(output, /"viewport":"1280x800"/u);
  assert.match(output, /"viewport":"1440x900"/u);
  assert.match(output, /"viewport":"1920x1080"/u);
  assert.match(output, /"state":"empty"/u);
  assert.match(output, /"state":"selected-project"/u);
  assert.match(output, /"state":"active-conversation"/u);

  // Both skins shipped, but only the dark one had screenshot evidence, so a
  // light-skin regression had nothing to fail against. The harness proves the
  // surface token actually lightened before recording a capture — otherwise a
  // dark shot filed under "light" would read as evidence.
  assert.match(output, /"skin":"dark"/u);
  assert.match(output, /"skin":"light"/u);

  // The settings screen is reachable from the shell but no spec opened it.
  assert.match(output, /"state":"settings"/u);
});
