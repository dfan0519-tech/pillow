import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = fileURLToPath(new URL("../../../../", import.meta.url));
// Repository-local process supervisor (no published types).
// @ts-expect-error Task harness has no published TypeScript declarations.
const { runSupervisedCommand } = await import("../../../../scripts/lib/desktop-process.mjs");

const RECORD_PREFIX = "KEEPER_WAVE_C3_GOVERNED_WRITE_ACCEPTANCE";

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

test(
  "Wave C3-F proves governed write reject/approve/modify/drift/cancel/isolation/restart over real Tauri WebView2",
  {
    timeout: 1_800_000,
  },
  async () => {
    const result = await runSupervisedCommand(
      process.execPath,
      ["scripts/wave-c3-governed-write-acceptance.mjs"],
      {
        cwd: repositoryRoot,
        // Nested orchestrator owns its managed child lifecycle; the outer
        // wrapper still proves the desktop baseline is clean after completion.
        allowMsvcVctipTelemetry: true,
        enforceDesktopClean: true,
        env: process.env,
        timeoutMs: 1_740_000,
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
    assert.equal(record.processCleanup, "passed");
    assert.equal(record.normalClose, "passed");
    assert.equal(record.forcedRestartCleanup, "passed");

    const scenarios = record.scenarios as string[] | undefined;
    assert.ok(Array.isArray(scenarios), "scenarios must be present");
    for (const required of [
      "reject-create",
      "approve-create",
      "approve-modify",
      "base-drift",
      "cancellation",
      "conflict-replay",
      "two-projects",
      "forced-restart",
    ]) {
      assert.ok(
        scenarios.includes(required),
        `missing scenario ${required}: ${JSON.stringify(scenarios)}`,
      );
    }
  },
);
