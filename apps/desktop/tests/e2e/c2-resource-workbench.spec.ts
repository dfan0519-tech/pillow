import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = fileURLToPath(new URL("../../../../", import.meta.url));
// Repository-local process supervisor (no published types).
// @ts-expect-error Task harness has no published TypeScript declarations.
const { runSupervisedCommand } = await import("../../../../scripts/lib/desktop-process.mjs");

const RECORD_PREFIX = "KEEPER_WAVE_C2_RESOURCE_ACCEPTANCE";

function extractCompletionRecord(output: string): Record<string, unknown> {
  const line = output
    .split(/\r?\n/u)
    .map((entry) => entry.trim())
    .reverse()
    .find((entry) => entry.startsWith(`${RECORD_PREFIX} `));
  assert.ok(line, `Expected ${RECORD_PREFIX} terminal record in output.`);
  const payload = line.slice(RECORD_PREFIX.length).trim();
  const record = JSON.parse(payload) as Record<string, unknown>;
  assert.equal(typeof record, "object");
  assert.notEqual(record, null);
  return record;
}

test("Wave C2-E proves two-project isolation, restart recovery, and normal close over real Tauri WebView2", {
  timeout: 900_000,
}, async () => {
  const result = await runSupervisedCommand(
    process.execPath,
    ["scripts/wave-c2-resource-acceptance.mjs"],
    {
      cwd: repositoryRoot,
      // Nested orchestrator owns desktop process baseline and cleanup proofs.
      // MSVC spawns a detached vctip.exe telemetry process during the debug
      // build that routinely outlives the supervisor's cleanup grace. The
      // allowance is identity-gated and applies only after the root is
      // confirmed dead, so it cannot mask a live desktop process.
      allowMsvcVctipTelemetry: true,
      enforceDesktopClean: false,
      env: process.env,
      timeoutMs: 840_000,
    },
  );
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;

  assert.equal(result.status, 0, output);
  assert.equal(result.timedOut, false, output);
  assert.match(output, new RegExp(RECORD_PREFIX, "u"));

  const record = extractCompletionRecord(output);
  assert.equal(record.schemaVersion, 1);
  assert.equal(record.status, "passed");
  assert.equal(record.ipcBoundary, "real-tauri-webview");
  assert.equal(record.projectIsolation, "passed");
  assert.equal(record.restartRecovery, "passed");
  assert.equal(record.resourceCoverage, "tree-search-preview-recents");
  assert.equal(record.activityCoverage, "conversation-owned-real-events");
  assert.equal(record.keyboardReachable, "passed");
  assert.equal(record.normalClose, "passed");
  assert.equal(record.processCleanup, "passed");
  assert.deepEqual(record.phases, [
    "register-a",
    "register-b",
    "create",
    "restore",
  ]);
});
