import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = fileURLToPath(new URL("../../../../", import.meta.url));
// The acceptance supervisor is intentionally repository-local and executable by Node.
// @ts-expect-error The Task 5 Node helper has no published TypeScript declarations.
const { runSupervisedCommand } = await import("../../../../scripts/lib/desktop-process.mjs");

async function runNode(args: string[], timeoutMs = 300_000) {
  return runSupervisedCommand(process.execPath, args, {
    cwd: repositoryRoot,
    env: process.env,
    timeoutMs,
  });
}

test("creates, selects, and restores a project through Tauri IPC and SQLite", {
  timeout: 330_000,
}, async () => {
  const result = await runNode(["scripts/native-acceptance-desktop.mjs"]);
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;

  assert.equal(result.status, 0, output);
  assert.equal(result.timedOut, false, output);
  assert.match(output, /KEEPER_NATIVE_ACCEPTANCE /u);
  assert.match(output, /"restoredProjectCount":1/u);
  assert.match(output, /"ipcBoundary":"real-tauri-webview"/u);
  assert.match(output, /"webviewProfile":"temporary-and-populated"/u);
});

test("desktop smoke harness proves readiness polling and process-tree cleanup", async () => {
  const result = await runNode(
    ["scripts/smoke-desktop.mjs", "--self-test"],
    60_000,
  );
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;

  assert.equal(result.status, 0, output);
  assert.equal(result.timedOut, false, output);
  assert.match(output, /KEEPER_DESKTOP_SMOKE_SELF_TEST /u);
  assert.match(output, /"readinessContract":"passed"/u);
  assert.match(output, /"webviewProfileIsolation":"passed"/u);
  assert.match(output, /"cleanupSuccess":"passed"/u);
  assert.match(output, /"cleanupFailure":"passed"/u);
  assert.match(output, /"cleanupTimeout":"passed"/u);
  assert.match(output, /"cleanupParentExited":"passed"/u);
});

test("cargo wrapper preserves exits and cleans its child tree on signals", async () => {
  const result = await runNode(["scripts/run-cargo.mjs", "--self-test"], 60_000);
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;

  assert.equal(result.status, 0, output);
  assert.equal(result.timedOut, false, output);
  assert.match(output, /KEEPER_RUN_CARGO_SELF_TEST /u);
  assert.match(output, /"exitCode":"passed"/u);
  assert.match(output, /"signalCleanup":"passed"/u);
  assert.match(output, /"msvcToolchain":"passed"/u);
});
