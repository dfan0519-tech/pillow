import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = fileURLToPath(new URL("../../../../", import.meta.url));
// Repository-local process supervisor (no published types).
// @ts-expect-error Task harness has no published TypeScript declarations.
const { runSupervisedCommand } = await import("../../../../scripts/lib/desktop-process.mjs");

const RECORD_PREFIX = "KEEPER_WAVE_C4_GIT_CHANGES_ACCEPTANCE";

const REQUIRED_SCENARIOS = [
  "project-root-clean",
  "nested-repository",
  "modified-untracked",
  "rename",
  "conflict",
  "detached-head",
  "text-diff",
  "binary-oversized-diff",
  "unsafe-config-denial",
  "generation-drift",
  "project-switch-isolation",
  "restart-recovery",
] as const;

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
  "Wave C4 D3B4 proves real desktop Git discovery/status/diff isolation over production Tauri WebView2",
  {
    // Real debug build + multi-scenario Git E2E; no artificial wall-clock cutoff.
    timeout: 2_400_000,
  },
  async () => {
    const result = await runSupervisedCommand(
      process.execPath,
      ["scripts/wave-c4-git-changes-acceptance.mjs"],
      {
        cwd: repositoryRoot,
        // Nested orchestrator owns its managed child lifecycle; the outer
        // wrapper still proves the desktop baseline is clean after completion.
        allowMsvcVctipTelemetry: true,
        enforceDesktopClean: true,
        env: process.env,
        timeoutMs: 2_340_000,
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
    assert.equal(record.runtimePath, "production-tauri-webview-real-host-git");
    assert.equal(record.gitPath, "GitCommandState->GitReadExecutor");
    assert.equal(record.processCleanup, "passed");
    assert.equal(record.normalClose, "passed");
    assert.equal(record.restartRecovery, "passed");
    assert.equal(record.residualProcesses, 0);

    const viewports = record.viewports as string[] | undefined;
    assert.ok(Array.isArray(viewports), "viewports must be present");
    assert.ok(viewports.includes("1440x960"), "missing 1440x960 viewport");
    assert.ok(viewports.includes("1024x720"), "missing 1024x720 viewport");

    const scenarios = record.scenarios as string[] | undefined;
    assert.ok(Array.isArray(scenarios), "scenarios must be present");
    for (const required of REQUIRED_SCENARIOS) {
      assert.ok(
        scenarios.includes(required),
        `missing scenario ${required}: ${JSON.stringify(scenarios)}`,
      );
    }
  },
);
