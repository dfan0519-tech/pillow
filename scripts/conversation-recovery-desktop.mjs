/**
 * Slice B1 conversation recovery acceptance.
 * Phase create_stream: real Tauri WebView IPC creates a conversation, sends a
 * deterministic fake turn, waits for completion, writes evidence.
 * Process tree is fully cleaned. Phase restore: same temporary SQLite app-data
 * is reopened; messages, events and run status are restored.
 */
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { mkdtemp, mkdir, rm, stat } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  ProcessSupervisor,
  assertDesktopBaselineClean,
  captureDesktopBaseline,
  observeProcessSurvival,
  runChecked,
  startCaptured,
  waitForJsonFile,
} from "./lib/desktop-process.mjs";
import {
  assertPathSnapshotUnchanged,
  defaultWebViewProfilePath,
  snapshotPath,
} from "./lib/desktop-profile.mjs";
import { validateReadinessEvidence } from "./lib/desktop-readiness.mjs";
import { getMsvcEnvironment } from "./lib/msvc-env.mjs";

const repositoryRoot = fileURLToPath(new URL("..", import.meta.url));
const packageManager = process.platform === "win32" ? "cmd.exe" : "pnpm";
const packageManagerArgs = (args) =>
  process.platform === "win32"
    ? ["/d", "/s", "/c", "pnpm", ...args]
    : args;
const executable = path.join(
  repositoryRoot,
  "target",
  "debug",
  process.platform === "win32" ? "keeper-ide.exe" : "keeper-ide",
);

function validateConversationEvidence(value, expected) {
  assert.equal(value.schemaVersion, 1);
  assert.equal(value.phase, expected.phase);
  assert.equal(value.status, "passed", value.errorMessage ?? "acceptance failed");
  assert.equal(value.ipcBoundary, "real-tauri-webview");
  assert.equal(typeof value.projectId, "string");
  assert.ok(value.projectId.length > 0);
  assert.equal(typeof value.conversationId, "string");
  assert.ok(value.conversationId.length > 0);
  assert.equal(typeof value.runId, "string");
  assert.ok(value.runId.length > 0);
  assert.equal(typeof value.runStatus, "string");
  assert.ok(value.runStatus.length > 0);
  assert.equal(typeof value.eventCount, "number");
  assert.ok(value.eventCount >= 1);
  assert.equal(typeof value.pid, "number");
  assert.ok(value.pid > 0);
}

async function runPhase({
  phase,
  appDataDir,
  webviewProfileDir,
  projectName,
  projectRoot,
  prompt,
  environment,
}) {
  const readinessPath = path.join(appDataDir, "desktop-ready.json");
  const resultPath = path.join(appDataDir, `conversation-acceptance-${phase}.json`);
  await rm(readinessPath, { force: true });
  await rm(resultPath, { force: true });
  const baseline = captureDesktopBaseline();
  assertDesktopBaselineClean(baseline);
  const nonce = randomUUID();
  const launchLowerBound = Date.now();
  const processRun = startCaptured(executable, [], {
    cwd: repositoryRoot,
    env: {
      ...environment,
      KEEPER_IDE_CONVERSATION_ACCEPTANCE_PHASE: phase,
      KEEPER_IDE_CONVERSATION_ACCEPTANCE_PROMPT: prompt,
      KEEPER_IDE_CONVERSATION_ACCEPTANCE_RESULT: resultPath,
      KEEPER_IDE_ACCEPTANCE_PROJECT_NAME: projectName,
      KEEPER_IDE_ACCEPTANCE_PROJECT_ROOT: projectRoot,
      KEEPER_IDE_APP_DATA_DIR: appDataDir,
      KEEPER_IDE_LAUNCH_NONCE: nonce,
      KEEPER_IDE_STARTED_AT: String(launchLowerBound),
      WEBVIEW2_USER_DATA_FOLDER: webviewProfileDir,
    },
  });
  const supervisor = new ProcessSupervisor({
    baseline,
    rootPid: processRun.child.pid,
  });

  try {
    const expectedReadiness = {
      appDataDir,
      launchLowerBound,
      nonce,
      webviewProfileDir,
    };
    const readiness = await waitForJsonFile(readinessPath, {
      label: `conversation acceptance ${phase} readiness`,
      onPoll: () => supervisor.capture(),
      processHandle: processRun.child,
      timeoutMs: 60_000,
      validate: (value) => validateReadinessEvidence(value, expectedReadiness),
    });
    assert.equal(readiness.pid, processRun.child.pid);
    supervisor.trackPid(readiness.pid);
    await observeProcessSurvival(readiness.pid, 500);

    const evidence = await waitForJsonFile(resultPath, {
      label: `conversation acceptance ${phase} result`,
      onPoll: () => supervisor.capture(),
      processHandle: processRun.child,
      timeoutMs: 60_000,
      validate: (value) =>
        validateConversationEvidence(value, { phase }),
    });
    assert.equal(evidence.pid, readiness.pid);
    return evidence;
  } catch (error) {
    const output = processRun.output();
    throw new Error(
      `${error instanceof Error ? error.message : String(error)}\n`
      + `stdout:\n${output.stdout}\nstderr:\n${output.stderr}`,
    );
  } finally {
    await supervisor.cleanup();
  }
}

const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "keeper-ide-conv-"));
const appDataDir = path.join(temporaryRoot, "app-data");
const webviewProfileDir = path.join(temporaryRoot, "webview-profile");
const projectRoot = path.join(temporaryRoot, "project");
const projectName = "Keeper IDE Conversation Recovery";
const prompt = "summarize the workspace";
const realProfile = defaultWebViewProfilePath();
const realProfileBefore = await snapshotPath(realProfile);
const startedAt = performance.now();
let summary;

try {
  await mkdir(appDataDir, { recursive: true });
  await mkdir(webviewProfileDir, { recursive: true });
  await mkdir(projectRoot, { recursive: true });
  const environment = await getMsvcEnvironment();
  await runChecked(
    packageManager,
    packageManagerArgs([
      "--filter",
      "@keeper-ide/desktop",
      "tauri",
      "build",
      "--debug",
      "--no-bundle",
    ]),
    { cwd: repositoryRoot, env: environment },
  );

  const createEvidence = await runPhase({
    phase: "create_stream",
    appDataDir,
    webviewProfileDir,
    projectName,
    projectRoot,
    prompt,
    environment,
  });
  assert.equal(createEvidence.runStatus, "completed");
  assert.ok(createEvidence.eventCount >= 5);

  const profileEntriesAfterCreate = await snapshotPath(webviewProfileDir);
  assert.ok(
    profileEntriesAfterCreate.length > 0,
    "Temporary WebView2 profile was not populated.",
  );

  const restoreEvidence = await runPhase({
    phase: "restore",
    appDataDir,
    webviewProfileDir,
    projectName,
    projectRoot,
    prompt,
    environment,
  });
  assert.equal(restoreEvidence.projectId, createEvidence.projectId);
  assert.equal(restoreEvidence.conversationId, createEvidence.conversationId);
  assert.equal(restoreEvidence.runId, createEvidence.runId);
  assert.equal(restoreEvidence.runStatus, "completed");
  assert.equal(restoreEvidence.eventCount, createEvidence.eventCount);

  const databasePath = path.join(appDataDir, "keeper.db");
  const dbStat = await stat(databasePath);
  assert.ok(dbStat.size > 0, "SQLite database must exist after recovery");

  summary = {
    status: "passed",
    createEventCount: createEvidence.eventCount,
    restoredEventCount: restoreEvidence.eventCount,
    runStatus: restoreEvidence.runStatus,
    databaseBytes: dbStat.size,
    webviewProfile: "temporary-and-populated",
    webviewProfileEntries: profileEntriesAfterCreate.length,
    durationMs: Math.round(performance.now() - startedAt),
    ipcBoundary: "real-tauri-webview",
  };
} catch (error) {
  summary = {
    status: "failed",
    error: error instanceof Error ? error.message : String(error),
    durationMs: Math.round(performance.now() - startedAt),
  };
  process.exitCode = 1;
} finally {
  const realProfileAfter = await snapshotPath(realProfile);
  try {
    assertPathSnapshotUnchanged(
      realProfileBefore,
      realProfileAfter,
      "real LocalAppData WebView2 profile",
    );
  } catch (error) {
    summary = {
      ...(summary ?? { status: "failed" }),
      status: "failed",
      error: error instanceof Error ? error.message : String(error),
    };
    process.exitCode = 1;
  }
  await rm(temporaryRoot, {
    recursive: true,
    force: true,
    maxRetries: 20,
    retryDelay: 100,
  }).catch(() => undefined);
}

if (summary?.status === "passed") {
  console.log(`KEEPER_CONVERSATION_RECOVERY ${JSON.stringify(summary)}`);
} else {
  console.error(`KEEPER_CONVERSATION_RECOVERY ${JSON.stringify(summary)}`);
}
