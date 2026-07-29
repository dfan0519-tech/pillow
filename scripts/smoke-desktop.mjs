import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, stat, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  isProcessAlive,
  runChecked,
  startCaptured,
  terminateProcessTree,
  waitForJsonFile,
  waitForOutputMatch,
} from "./lib/desktop-process.mjs";
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

function validateReadiness(value, appDataDir) {
  assert.equal(value.schemaVersion, 1);
  assert.equal(value.status, "ready");
  assert.ok(Number.isInteger(value.pid) && value.pid > 0);
  assert.equal(path.resolve(value.appDataDir), path.resolve(appDataDir));
  assert.equal(path.resolve(value.databasePath), path.join(path.resolve(appDataDir), "keeper.db"));
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

async function selfTest() {
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "keeper-smoke-self-"));
  const readinessPath = path.join(temporaryRoot, "ready.json");
  const expected = { schemaVersion: 1, status: "ready" };
  const writer = setTimeout(() => {
    void writeFile(readinessPath, JSON.stringify(expected), "utf8");
  }, 75);

  let parentPid;
  let childPid;
  try {
    const readiness = await waitForJsonFile(readinessPath, {
      timeoutMs: 2_000,
      label: "self-test",
      validate: (value) => assert.deepEqual(value, expected),
    });
    assert.deepEqual(readiness, expected);

    const nestedScript = [
      'const { spawn } = require("node:child_process");',
      'const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"],',
      '{ stdio: "ignore", windowsHide: true });',
      'console.log(`TREE_CHILD=${child.pid}`);',
      "setInterval(() => {}, 1000);",
    ].join("");
    const processRun = startCaptured(process.execPath, ["-e", nestedScript]);
    parentPid = processRun.child.pid;
    const match = await waitForOutputMatch(processRun, /TREE_CHILD=(\d+)/u, 2_000);
    childPid = Number(match[1]);
    assert.ok(isProcessAlive(parentPid));
    assert.ok(isProcessAlive(childPid));

    await terminateProcessTree(parentPid);
    assert.equal(isProcessAlive(parentPid), false);
    assert.equal(isProcessAlive(childPid), false);

    console.log(
      `KEEPER_DESKTOP_SMOKE_SELF_TEST ${JSON.stringify({
        processTreeCleanup: "passed",
        readiness: "passed",
      })}`,
    );
  } finally {
    clearTimeout(writer);
    if (parentPid) await terminateProcessTree(parentPid);
    if (childPid) await terminateProcessTree(childPid);
    await rm(temporaryRoot, {
      recursive: true,
      force: true,
      maxRetries: 20,
      retryDelay: 100,
    });
  }
}

async function smoke(mode) {
  const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), `keeper-ide-${mode}-`));
  const appDataDir = path.join(temporaryRoot, "app-data");
  const readinessPath = path.join(appDataDir, "desktop-ready.json");
  const environment = await getMsvcEnvironment();
  let processRun;
  let readiness;
  let summary;

  try {
    await mkdir(appDataDir, { recursive: true });
    if (mode === "release") {
      await runChecked(
        packageManager,
        packageManagerArgs([
          "--filter",
          "@keeper-ide/desktop",
          "tauri",
          "build",
          "--no-bundle",
        ]),
        { cwd: repositoryRoot, env: environment },
      );
    }

    const command =
      mode === "release"
        ? path.join(repositoryRoot, "target", "release", "keeper-ide.exe")
        : packageManager;
    const args =
      mode === "release"
        ? []
        : packageManagerArgs(["--filter", "@keeper-ide/desktop", "tauri", "dev"]);
    const startedAt = performance.now();
    processRun = startCaptured(command, args, {
      cwd: repositoryRoot,
      env: {
        ...(mode === "dev" ? devLoopbackEnvironment(environment) : environment),
        KEEPER_IDE_APP_DATA_DIR: appDataDir,
      },
    });
    readiness = await waitForJsonFile(readinessPath, {
      timeoutMs: mode === "release" ? 15_000 : 60_000,
      label: `${mode} desktop`,
      processHandle: processRun.child,
      validate: (value) => validateReadiness(value, appDataDir),
    });
    const startupMs = Math.round(performance.now() - startedAt);
    const database = await stat(path.join(appDataDir, "keeper.db"));
    assert.ok(database.isFile() && database.size > 0);
    if (mode === "release" && startupMs > 2_500) {
      throw new Error(`Release startup ${startupMs} ms exceeds the 2500 ms budget.`);
    }

    summary = {
      appDataWasTemporary: appDataDir.startsWith(temporaryRoot),
      databaseBytes: database.size,
      mode,
      readiness: "renderer-command-marker",
      startupBudgetMs: mode === "release" ? 2500 : null,
      startupMs,
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
    if (processRun?.child.pid) await terminateProcessTree(processRun.child.pid);
    if (readiness?.pid && readiness.pid !== processRun?.child.pid) {
      await terminateProcessTree(readiness.pid);
    }
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
  await selfTest();
} else {
  await smoke(modeFromArguments(process.argv.slice(2)));
}
