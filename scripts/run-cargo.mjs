import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { spawn } from "node:child_process";
import {
  ProcessSupervisor,
  delay,
  isProcessAlive,
} from "./lib/desktop-process.mjs";
import {
  getMsvcEnvironment,
  getMsvcEnvironmentDetails,
} from "./lib/msvc-env.mjs";

const repositoryRoot = new URL("..", import.meta.url);
const signalExitCode = {
  SIGINT: 130,
  SIGTERM: 143,
};

async function runManagedChild(
  command,
  args,
  {
    cwd = repositoryRoot,
    environment = process.env,
    onStdout,
    signalSource = process,
    stdio = "inherit",
  } = {},
) {
  const child = spawn(command, args, {
    cwd,
    env: environment,
    stdio,
    windowsHide: true,
  });
  const supervisor = new ProcessSupervisor({
    baseline: { port: 1420 },
    child,
    enforceDesktopClean: false,
  });
  const captureTimer = setInterval(() => {
    void supervisor.capture();
  }, 500);
  let requestedSignal = null;
  let resolveRequestedSignal;
  const requestedSignalPromise = new Promise((resolve) => {
    resolveRequestedSignal = resolve;
  });
  const requestSignal = (signal) => {
    if (requestedSignal) return;
    requestedSignal = signal;
    resolveRequestedSignal(signal);
  };
  const onSigint = () => requestSignal("SIGINT");
  const onSigterm = () => requestSignal("SIGTERM");
  signalSource.on("SIGINT", onSigint);
  signalSource.on("SIGTERM", onSigterm);
  if (child.stdout) {
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => onStdout?.(chunk));
  }

  const exitPromise = new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("exit", (code, signal) => resolve({ code, signal }));
  });

  try {
    const first = await Promise.race([
      exitPromise.then((result) => ({ kind: "exit", result })),
      requestedSignalPromise.then((signal) => ({ kind: "signal", signal })),
    ]);
    if (first.kind === "exit") {
      await supervisor.cleanup();
      return first.result.signal
        ? signalExitCode[first.result.signal] ?? 1
        : first.result.code ?? 1;
    }

    await supervisor.capture(true);
    if (isProcessAlive(child.pid)) child.kill(first.signal);
    await Promise.race([exitPromise, delay(2_000)]);
    await supervisor.cleanup();
    return signalExitCode[first.signal];
  } finally {
    clearInterval(captureTimer);
    signalSource.off("SIGINT", onSigint);
    signalSource.off("SIGTERM", onSigterm);
  }
}

async function selfTest() {
  const quietSignals = new EventEmitter();
  const preservedExit = await runManagedChild(
    process.execPath,
    ["-e", "setTimeout(() => process.exit(7), 2_000)"],
    { signalSource: quietSignals, stdio: "ignore" },
  );
  assert.equal(preservedExit, 7);

  const signalSource = new EventEmitter();
  let descendantPid;
  let output = "";
  const nestedScript = [
    'const { spawn } = require("node:child_process");',
    'const child = spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"],',
    '{ stdio: "ignore", windowsHide: true });',
    'console.log(`CARGO_DESCENDANT=${child.pid}`);',
    "setInterval(() => {}, 1000);",
  ].join("");
  const signaledExit = await runManagedChild(
    process.execPath,
    ["-e", nestedScript],
    {
      onStdout: (chunk) => {
        output += chunk;
        const match = output.match(/CARGO_DESCENDANT=(\d+)/u);
        if (!match || descendantPid) return;
        descendantPid = Number(match[1]);
        setTimeout(() => signalSource.emit("SIGTERM"), 75);
      },
      signalSource,
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  assert.equal(signaledExit, 143);
  assert.ok(descendantPid);
  assert.equal(isProcessAlive(descendantPid), false);

  const msvc = await getMsvcEnvironmentDetails();
  if (process.platform === "win32") {
    assert.match(msvc.clPath, /cl\.exe$/iu);
    assert.match(msvc.linkPath, /link\.exe$/iu);
    assert.match(
      msvc.discovery,
      /^(?:vswhere-with-vc-component|direct-verified-fallback)$/u,
    );
  }

  console.log(
    `KEEPER_RUN_CARGO_SELF_TEST ${JSON.stringify({
      exitCode: "passed",
      msvcDiscovery: msvc.discovery,
      msvcToolchain: "passed",
      signalCleanup: "passed",
    })}`,
  );
}

const args = process.argv.slice(2);
if (args.includes("--self-test")) {
  await selfTest();
} else {
  if (args.length === 0) {
    console.error("Usage: node scripts/run-cargo.mjs <cargo arguments>");
    process.exit(2);
  }
  const exitCode = await runManagedChild("cargo", args, {
    environment: await getMsvcEnvironment(),
  });
  process.exit(exitCode);
}
