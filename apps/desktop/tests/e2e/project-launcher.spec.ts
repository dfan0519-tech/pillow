import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = fileURLToPath(new URL("../../../../", import.meta.url));

function runNode(args: string[]) {
  return spawnSync(process.execPath, args, {
    cwd: repositoryRoot,
    encoding: "utf8",
    env: process.env,
    maxBuffer: 16 * 1024 * 1024,
    timeout: 300_000,
    windowsHide: true,
  });
}

test("creates, selects, and restores a project through Tauri IPC and SQLite", () => {
  const result = runNode(["scripts/native-acceptance-desktop.mjs"]);
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;

  assert.equal(result.status, 0, output);
  assert.match(output, /KEEPER_NATIVE_ACCEPTANCE /u);
  assert.match(output, /"restoredProjectCount":1/u);
  assert.match(output, /"ipcBoundary":"real-tauri-webview"/u);
});

test("desktop smoke harness proves readiness polling and process-tree cleanup", () => {
  const result = runNode(["scripts/smoke-desktop.mjs", "--self-test"]);
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;

  assert.equal(result.status, 0, output);
  assert.match(output, /KEEPER_DESKTOP_SMOKE_SELF_TEST /u);
  assert.match(output, /"readiness":"passed"/u);
  assert.match(output, /"processTreeCleanup":"passed"/u);
});
