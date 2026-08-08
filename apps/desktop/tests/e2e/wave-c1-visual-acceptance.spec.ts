import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = fileURLToPath(new URL("../../../../", import.meta.url));
// @ts-expect-error Repository-local supervisor has no declaration file.
const { runSupervisedCommand } = await import("../../../../scripts/lib/desktop-process.mjs");

test("Wave C1 visual acceptance captures every desktop state without geometry or interaction regressions", {
  timeout: 180_000,
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
      timeoutMs: 150_000,
    },
  );
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;

  assert.equal(result.status, 0, output);
  assert.equal(result.timedOut, false, output);
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
