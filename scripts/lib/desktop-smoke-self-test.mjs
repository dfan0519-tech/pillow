import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import {
  mkdtemp,
  mkdir,
  rm,
  writeFile,
} from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import {
  ProcessSupervisor,
  assertDesktopBaselineClean,
  captureDesktopBaseline,
  isProcessAlive,
  observeProcessSurvival,
  runSupervisedCommand,
  startCaptured,
  waitForJsonFile,
  waitForOutputMatch,
} from "./desktop-process.mjs";
import {
  assertPathSnapshotUnchanged,
  defaultWebViewProfilePath,
  snapshotPath,
} from "./desktop-profile.mjs";
import { validateReadinessEvidence } from "./desktop-readiness.mjs";

function nestedProcessScript({ exitParent = false, listen = false } = {}) {
  const childSource = "setInterval(() => {}, 1000)";
  return [
    'const { spawn } = require("node:child_process");',
    listen ? 'require("node:net").createServer(() => {}).listen(1420);' : "",
    `const child = spawn(process.execPath, ["-e", ${JSON.stringify(childSource)}],`,
    `{ detached: ${exitParent}, stdio: "ignore", windowsHide: true });`,
    'console.log(`TREE_CHILD=${child.pid}`);',
    exitParent ? "setTimeout(() => process.exit(0), 75);" : "setInterval(() => {}, 1000);",
  ].join("");
}

async function cleanupCase({ action, exitParent = false, listen = false }) {
  const baseline = captureDesktopBaseline();
  assertDesktopBaselineClean(baseline);
  const processRun = startCaptured(
    process.execPath,
    ["-e", nestedProcessScript({ exitParent, listen })],
  );
  const parentPid = processRun.child.pid;
  const supervisor = new ProcessSupervisor({
    baseline,
    rootPid: parentPid,
  });
  let childPid;
  let actionError;
  try {
    const match = await waitForOutputMatch(
      processRun,
      /TREE_CHILD=(\d+)/u,
      3_000,
      () => supervisor.capture(),
    );
    childPid = Number(match[1]);
    supervisor.trackPid(childPid);
    await supervisor.capture(true);
    await action({ processRun, supervisor });
  } catch (error) {
    actionError = error;
  } finally {
    await supervisor.cleanup();
  }
  assert.equal(isProcessAlive(parentPid), false);
  assert.equal(isProcessAlive(childPid), false);
  if (actionError) throw actionError;
}

export async function runDesktopSmokeSelfTest() {
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "keeper-smoke-self-"));
  const realProfile = defaultWebViewProfilePath();
  const realProfileBefore = await snapshotPath(realProfile);
  const webviewProfileDir = path.join(temporaryRoot, "webview-profile");
  const appDataDir = path.join(temporaryRoot, "app-data");
  const readinessPath = path.join(appDataDir, "ready.json");
  const nonce = randomUUID();
  let readinessSupervisor;

  try {
    await mkdir(appDataDir, { recursive: true });
    await mkdir(webviewProfileDir, { recursive: true });
    await writeFile(path.join(webviewProfileDir, "profile-created"), "ok", "utf8");
    const launchLowerBound = Date.now();
    const readinessProcess = startCaptured(
      process.execPath,
      ["-e", "setInterval(() => {}, 1000)"],
    );
    readinessSupervisor = new ProcessSupervisor({
      baseline: captureDesktopBaseline(),
      enforceDesktopClean: false,
      rootPid: readinessProcess.child.pid,
    });
    const marker = {
      appDataDir,
      databasePath: path.join(appDataDir, "keeper.db"),
      nonce,
      pid: readinessProcess.child.pid,
      readyAtUnixMs: Date.now() + 20,
      schemaVersion: 2,
      startedAt: launchLowerBound,
      status: "ready",
      webviewProfileDir,
    };
    setTimeout(() => {
      void writeFile(readinessPath, JSON.stringify(marker), "utf8");
    }, 75);
    const readiness = await waitForJsonFile(readinessPath, {
      label: "readiness contract self-test",
      onPoll: () => readinessSupervisor.capture(),
      processHandle: readinessProcess.child,
      timeoutMs: 2_000,
      validate: (value) => validateReadinessEvidence(value, {
        appDataDir,
        launchLowerBound,
        nonce,
        webviewProfileDir,
      }),
    });
    await observeProcessSurvival(readiness.pid, 250);
    await readinessSupervisor.cleanup();

    await cleanupCase({ action: async () => {}, listen: true });
    await assert.rejects(
      cleanupCase({
        action: async () => {
          throw new Error("expected self-test failure");
        },
      }),
      /expected self-test failure/u,
    );
    const timeoutResult = await runSupervisedCommand(
      process.execPath,
      ["-e", nestedProcessScript()],
      { timeoutMs: 250 },
    );
    assert.equal(timeoutResult.timedOut, true);
    assert.equal(timeoutResult.status, null);
    const timeoutChild = timeoutResult.stdout.match(/TREE_CHILD=(\d+)/u);
    assert.ok(timeoutChild);
    assert.equal(isProcessAlive(Number(timeoutChild[1])), false);
    await cleanupCase({
      action: async ({ processRun }) => {
        if (processRun.child.exitCode !== null) return;
        await new Promise((resolve, reject) => {
          processRun.child.once("error", reject);
          processRun.child.once("exit", resolve);
        });
      },
      exitParent: true,
    });

    const realProfileAfter = await snapshotPath(realProfile);
    assertPathSnapshotUnchanged(
      realProfileBefore,
      realProfileAfter,
      "real LocalAppData WebView2 profile",
    );
    console.log(
      `KEEPER_DESKTOP_SMOKE_SELF_TEST ${JSON.stringify({
        cleanupFailure: "passed",
        cleanupParentExited: "passed",
        cleanupSuccess: "passed",
        cleanupTimeout: "passed",
        readinessContract: "passed",
        webviewProfileIsolation: "passed",
      })}`,
    );
  } finally {
    if (readinessSupervisor) await readinessSupervisor.cleanup();
    await rm(temporaryRoot, {
      recursive: true,
      force: true,
      maxRetries: 20,
      retryDelay: 100,
    });
  }
}
