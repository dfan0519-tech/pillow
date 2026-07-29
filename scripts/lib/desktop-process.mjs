import { readFile } from "node:fs/promises";
import { spawn, spawnSync } from "node:child_process";

const POLL_INTERVAL_MS = 25;

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

export function isProcessAlive(pid) {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

export async function waitForJsonFile(
  file,
  { timeoutMs, validate, processHandle, label },
) {
  const startedAt = performance.now();
  let lastError = null;

  while (performance.now() - startedAt < timeoutMs) {
    if (processHandle && processHandle.exitCode !== null) {
      throw new Error(
        `${label} process exited with code ${processHandle.exitCode} before readiness.`,
      );
    }
    try {
      const value = JSON.parse(await readFile(file, "utf8"));
      validate(value);
      return value;
    } catch (error) {
      lastError = error;
    }
    await delay(POLL_INTERVAL_MS);
  }

  const detail = lastError instanceof Error ? ` Last observation: ${lastError.message}` : "";
  throw new Error(`${label} readiness timed out after ${timeoutMs} ms.${detail}`);
}

export function startCaptured(command, args, options = {}) {
  const { forwardOutput = false, ...spawnOptions } = options;
  const requiresCommandShell =
    process.platform === "win32" && /\.(?:cmd|bat)$/iu.test(command);
  const child = spawn(command, args, {
    ...spawnOptions,
    shell: spawnOptions.shell ?? requiresCommandShell,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  let stdout = "";
  let stderr = "";
  child.stdout.setEncoding("utf8");
  child.stderr.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    stdout += chunk;
    if (forwardOutput) process.stdout.write(chunk);
  });
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
    if (forwardOutput) process.stderr.write(chunk);
  });
  return {
    child,
    output: () => ({ stdout, stderr }),
  };
}

export async function runChecked(command, args, options = {}) {
  const processRun = startCaptured(command, args, {
    ...options,
    forwardOutput: true,
  });
  const code = await new Promise((resolve, reject) => {
    processRun.child.once("error", reject);
    processRun.child.once("exit", (exitCode, signal) => {
      if (signal) reject(new Error(`${command} terminated by ${signal}.`));
      else resolve(exitCode);
    });
  });
  if (code !== 0) {
    const output = processRun.output();
    throw new Error(
      `${command} exited with code ${code}.\n${output.stderr || output.stdout}`,
    );
  }
}

export async function terminateProcessTree(pid) {
  if (!isProcessAlive(pid)) return;

  if (process.platform === "win32") {
    spawnSync("taskkill.exe", ["/PID", String(pid), "/T", "/F"], {
      encoding: "utf8",
      windowsHide: true,
    });
  } else {
    try {
      process.kill(-pid, "SIGKILL");
    } catch {
      process.kill(pid, "SIGKILL");
    }
  }

  const deadline = performance.now() + 5_000;
  while (isProcessAlive(pid) && performance.now() < deadline) {
    await delay(POLL_INTERVAL_MS);
  }
  if (isProcessAlive(pid)) {
    throw new Error(`Process tree rooted at PID ${pid} survived cleanup.`);
  }
}

export async function waitForOutputMatch(processRun, pattern, timeoutMs) {
  const deadline = performance.now() + timeoutMs;
  while (performance.now() < deadline) {
    const match = processRun.output().stdout.match(pattern);
    if (match) return match;
    if (processRun.child.exitCode !== null) {
      throw new Error("Process exited before emitting the expected output.");
    }
    await delay(POLL_INTERVAL_MS);
  }
  throw new Error(`Timed out waiting for process output matching ${pattern}.`);
}
