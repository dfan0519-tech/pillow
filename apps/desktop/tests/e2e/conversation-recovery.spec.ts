import assert from "node:assert/strict";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = fileURLToPath(new URL("../../../../", import.meta.url));
// Repository-local process supervisor (no published types).
// @ts-expect-error Task harness has no published TypeScript declarations.
const { runSupervisedCommand } = await import("../../../../scripts/lib/desktop-process.mjs");

async function runNode(args: string[], timeoutMs = 600_000) {
  return runSupervisedCommand(process.execPath, args, {
    cwd: repositoryRoot,
    env: process.env,
    timeoutMs,
  });
}

async function runCargo(args: string[], timeoutMs = 300_000) {
  return runSupervisedCommand(process.execPath, ["scripts/run-cargo.mjs", ...args], {
    cwd: repositoryRoot,
    env: process.env,
    timeoutMs,
  });
}

test("conversation service proves persistence-before-emit, cancel, and Interrupted recovery", {
  timeout: 300_000,
}, async () => {
  const result = await runCargo([
    "test",
    "-p",
    "pillow-desktop",
    "conversation_service::tests",
    "--",
    "--nocapture",
  ]);
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;
  assert.equal(result.status, 0, output);
  assert.equal(result.timedOut, false, output);
  assert.match(output, /persistence_commits_before_renderer_emit/u);
  assert.match(output, /cancellation_yields_one_terminal_and_no_residual_slots/u);
  assert.match(output, /restart_marks_inflight_interrupted_idempotently/u);
  assert.match(output, /projects_retain_isolated_conversation_lists/u);
});

test("restarts a real Tauri desktop and restores conversation timeline from SQLite", {
  timeout: 630_000,
}, async () => {
  const result = await runNode(["scripts/conversation-recovery-desktop.mjs"]);
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`;

  assert.equal(result.status, 0, output);
  assert.equal(result.timedOut, false, output);
  assert.match(output, /KEEPER_CONVERSATION_RECOVERY /u);
  assert.match(output, /"status":"passed"/u);
  assert.match(output, /"ipcBoundary":"real-tauri-webview"/u);
  assert.match(output, /"runStatus":"completed"/u);
  assert.match(output, /"createEventCount":/u);
  assert.match(output, /"restoredEventCount":/u);
});
