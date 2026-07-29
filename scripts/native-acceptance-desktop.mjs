import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, stat } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  runChecked,
  startCaptured,
  terminateProcessTree,
  waitForJsonFile,
} from "./lib/desktop-process.mjs";
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

function validateEvidence(value, phase, appDataDir) {
  assert.equal(value.schemaVersion, 1);
  assert.equal(value.phase, phase);
  assert.equal(value.status, "passed", value.errorMessage ?? "acceptance failed");
  assert.equal(value.ipcBoundary, "real-tauri-webview");
  assert.equal(path.resolve(value.appDataDir), path.resolve(appDataDir));
  assert.equal(path.resolve(value.databasePath), path.join(path.resolve(appDataDir), "keeper.db"));
  assert.ok(Number.isInteger(value.pid) && value.pid > 0);
  assert.equal(typeof value.projectId, "string");
  assert.ok(value.projectId.length > 0);
}

async function runPhase({
  phase,
  appDataDir,
  projectName,
  projectRoot,
  environment,
}) {
  const resultPath = path.join(appDataDir, `native-acceptance-${phase}.json`);
  const processRun = startCaptured(executable, [], {
    cwd: repositoryRoot,
    env: {
      ...environment,
      KEEPER_IDE_APP_DATA_DIR: appDataDir,
      KEEPER_IDE_ACCEPTANCE_PHASE: phase,
      KEEPER_IDE_ACCEPTANCE_RESULT: resultPath,
      KEEPER_IDE_ACCEPTANCE_PROJECT_NAME: projectName,
      KEEPER_IDE_ACCEPTANCE_PROJECT_ROOT: projectRoot,
    },
  });

  let evidence;
  try {
    evidence = await waitForJsonFile(resultPath, {
      timeoutMs: 30_000,
      label: `native acceptance ${phase}`,
      processHandle: processRun.child,
      validate: (value) => validateEvidence(value, phase, appDataDir),
    });
    return evidence;
  } catch (error) {
    const output = processRun.output();
    throw new Error(
      `${error instanceof Error ? error.message : String(error)}\n`
      + `stdout:\n${output.stdout}\nstderr:\n${output.stderr}`,
    );
  } finally {
    await terminateProcessTree(processRun.child.pid);
    if (evidence?.pid && evidence.pid !== processRun.child.pid) {
      await terminateProcessTree(evidence.pid);
    }
  }
}

const temporaryRoot = await mkdtemp(path.join(os.tmpdir(), "keeper-ide-native-"));
const appDataDir = path.join(temporaryRoot, "app-data");
const projectRoot = path.join(temporaryRoot, "project");
const projectName = "Keeper IDE Native Acceptance";
const startedAt = performance.now();
let summary;

try {
  await mkdir(appDataDir, { recursive: true });
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

  const created = await runPhase({
    phase: "create",
    appDataDir,
    projectName,
    projectRoot,
    environment,
  });
  const database = await stat(path.join(appDataDir, "keeper.db"));
  assert.ok(database.isFile() && database.size > 0);

  const restored = await runPhase({
    phase: "restore",
    appDataDir,
    projectName,
    projectRoot,
    environment,
  });
  assert.equal(restored.projectId, created.projectId);
  assert.equal(restored.restoredProjectCount, 1);

  summary = {
    appDataWasTemporary: appDataDir.startsWith(temporaryRoot),
    databaseBytes: database.size,
    elapsedMs: Math.round(performance.now() - startedAt),
    ipcBoundary: restored.ipcBoundary,
    processRestarts: 1,
    restoredProjectCount: restored.restoredProjectCount,
  };
} finally {
  await rm(temporaryRoot, {
    recursive: true,
    force: true,
    maxRetries: 20,
    retryDelay: 100,
  });
}

console.log(`KEEPER_NATIVE_ACCEPTANCE ${JSON.stringify(summary)}`);
