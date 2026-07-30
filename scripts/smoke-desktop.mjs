import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import {
  mkdtemp,
  mkdir,
  rm,
  stat,
} from "node:fs/promises";
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
import { runDesktopSmokeSelfTest } from "./lib/desktop-smoke-self-test.mjs";
import { getMsvcEnvironment } from "./lib/msvc-env.mjs";

const repositoryRoot = fileURLToPath(new URL("..", import.meta.url));
const packageManager = process.platform === "win32" ? "cmd.exe" : "pnpm";
const packageManagerArgs = (args) =>
  process.platform === "win32"
    ? ["/d", "/s", "/c", "pnpm", ...args]
    : args;

function modeFromArguments(args) {
  const modeIndex = args.indexOf("--mode");
  if (modeIndex === -1) return "release";
  const mode = args[modeIndex + 1];
  if (mode !== "release" && mode !== "dev") {
    throw new Error("--mode must be release or dev.");
  }
  return mode;
}

function devLoopbackEnvironment(environment) {
  return {
    ...environment,
    ALL_PROXY: "",
    HTTPS_PROXY: "",
    HTTP_PROXY: "",
    NO_PROXY: "localhost,127.0.0.1,::1",
    all_proxy: "",
    https_proxy: "",
    http_proxy: "",
    no_proxy: "localhost,127.0.0.1,::1",
  };
}

async function smoke(mode) {
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), `pillow-${mode}-`));
  const appDataDir = path.join(temporaryRoot, "app-data");
  const webviewProfileDir = path.join(temporaryRoot, "webview-profile");
  const readinessPath = path.join(appDataDir, "desktop-ready.json");
  const realProfile = defaultWebViewProfilePath();
  const realProfileBefore = await snapshotPath(realProfile);
  const environment = await getMsvcEnvironment();
  let processRun;
  let supervisor;
  let readiness;
  let summary;

  try {
    await mkdir(appDataDir, { recursive: true });
    await mkdir(webviewProfileDir, { recursive: true });
    if (mode === "release") {
      await runChecked(
        packageManager,
        packageManagerArgs([
          "--filter",
          "@pillow/desktop",
          "tauri",
          "build",
          "--no-bundle",
        ]),
        { cwd: repositoryRoot, env: environment },
      );
    }

    const command = mode === "release"
      ? path.join(repositoryRoot, "target", "release", "pillow.exe")
      : packageManager;
    const args = mode === "release"
      ? []
      : packageManagerArgs(["--filter", "@pillow/desktop", "tauri", "dev"]);
    const baseline = captureDesktopBaseline();
    assertDesktopBaselineClean(baseline);
    const nonce = randomUUID();
    const launchLowerBound = Date.now();
    const startedAt = performance.now();
    processRun = startCaptured(command, args, {
      cwd: repositoryRoot,
      env: {
        ...(mode === "dev" ? devLoopbackEnvironment(environment) : environment),
        PILLOW_APP_DATA_DIR: appDataDir,
        PILLOW_LAUNCH_NONCE: nonce,
        PILLOW_STARTED_AT: String(launchLowerBound),
        WEBVIEW2_USER_DATA_FOLDER: webviewProfileDir,
      },
    });
    supervisor = new ProcessSupervisor({
      baseline,
      rootPid: processRun.child.pid,
    });
    readiness = await waitForJsonFile(readinessPath, {
      label: `${mode} desktop`,
      onPoll: () => supervisor.capture(),
      processHandle: processRun.child,
      timeoutMs: mode === "release" ? 15_000 : 60_000,
      validate: (value) => validateReadinessEvidence(value, {
        appDataDir,
        launchLowerBound,
        nonce,
        webviewProfileDir,
      }),
    });
    const observedReadyMs = Math.round(performance.now() - startedAt);
    const startupMs = Number(readiness.readyAtUnixMs - readiness.startedAt);
    await supervisor.capture(true);
    assert.equal(
      supervisor.hasTrackedPid(readiness.pid),
      true,
      `Readiness PID ${readiness.pid} was not in the captured launch tree.`,
    );
    supervisor.trackPid(readiness.pid);
    await observeProcessSurvival(readiness.pid, 500);
    await supervisor.capture(true);
    const database = await stat(path.join(appDataDir, "keeper.db"));
    assert.ok(database.isFile() && database.size > 0);
    const profileEntries = await snapshotPath(webviewProfileDir);
    assert.ok(profileEntries.length > 0, "Temporary WebView2 profile was not populated.");
    if (mode === "release" && startupMs > 2_500) {
      throw new Error(`Release startup ${startupMs} ms exceeds the 2500 ms budget.`);
    }

    summary = {
      appDataDir: "temporary",
      databaseBytes: database.size,
      databasePath: "temporary-app-data/keeper.db",
      mode,
      observedReadyMs,
      readiness: "list-projects-and-lifecycle-command",
      startupBudgetMs: mode === "release" ? 2500 : null,
      startupMs,
      webviewProfile: "temporary-and-populated",
      webviewProfileEntries: profileEntries.length,
    };
  } catch (error) {
    const output = processRun?.output() ?? { stdout: "", stderr: "" };
    console.error(
      `KEEPER_DESKTOP_SMOKE_FAILURE ${JSON.stringify({
        error: error instanceof Error ? error.message : String(error),
        mode,
        stderrTail: output.stderr.slice(-2000),
        stdoutTail: output.stdout.slice(-2000),
      })}`,
    );
    throw error;
  } finally {
    if (supervisor) await supervisor.cleanup();
    const realProfileAfter = await snapshotPath(realProfile);
    assertPathSnapshotUnchanged(
      realProfileBefore,
      realProfileAfter,
      "real LocalAppData WebView2 profile",
    );
    await rm(temporaryRoot, {
      recursive: true,
      force: true,
      maxRetries: 20,
      retryDelay: 100,
    });
  }

  console.log(`KEEPER_DESKTOP_SMOKE ${JSON.stringify(summary)}`);
}

if (process.argv.includes("--self-test")) {
  await runDesktopSmokeSelfTest();
} else {
  await smoke(modeFromArguments(process.argv.slice(2)));
}
