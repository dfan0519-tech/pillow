import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";

const POLL_INTERVAL_MS = 50;
const CLEANUP_ATTEMPTS = 4;

export function delay(milliseconds) {
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

function parseJsonArray(text) {
  const trimmed = text.trim();
  if (!trimmed) return [];
  const value = JSON.parse(trimmed);
  return Array.isArray(value) ? value : [value];
}

export function snapshotWindowsProcesses() {
  if (process.platform !== "win32") return [];
  const command = [
    "Get-CimInstance Win32_Process |",
    "ForEach-Object { [PSCustomObject]@{",
    "pid=[int]$_.ProcessId;",
    "parentPid=[int]$_.ParentProcessId;",
    "name=[string]$_.Name;",
    "executablePath=[string]$_.ExecutablePath",
    "} } | ConvertTo-Json -Compress",
  ].join(" ");
  const result = spawnSync(
    "powershell.exe",
    ["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", command],
    { encoding: "utf8", maxBuffer: 16 * 1024 * 1024, windowsHide: true },
  );
  if (result.status !== 0) {
    throw new Error(
      `Process snapshot failed with code ${result.status}: ${result.stderr}`,
    );
  }
  return parseJsonArray(result.stdout).map((entry) => ({
    executablePath: entry.executablePath ?? "",
    name: entry.name ?? "",
    parentPid: Number(entry.parentPid),
    pid: Number(entry.pid),
  }));
}

export function listenerPidsForPort(port = 1420) {
  if (process.platform !== "win32") return [];
  const result = spawnSync("netstat.exe", ["-ano", "-p", "TCP"], {
    encoding: "utf8",
    windowsHide: true,
  });
  if (result.status !== 0) {
    throw new Error(
      `Port snapshot failed with code ${result.status}: ${result.stderr}`,
    );
  }
  const pids = new Set();
  for (const line of result.stdout.split(/\r?\n/u)) {
    const columns = line.trim().split(/\s+/u);
    if (
      columns.length >= 5
      && columns[0] === "TCP"
      && columns[1].endsWith(`:${port}`)
      && columns[3] === "LISTENING"
    ) {
      const pid = Number(columns[4]);
      if (Number.isInteger(pid) && pid > 0) pids.add(pid);
    }
  }
  return [...pids];
}

function keeperProcessPids(processes) {
  return processes
    .filter((entry) => entry.name.toLowerCase() === "keeper-ide.exe")
    .map((entry) => entry.pid);
}

export function captureDesktopBaseline(port = 1420) {
  const processes = snapshotWindowsProcesses();
  return {
    keeperPids: keeperProcessPids(processes),
    listenerPids: listenerPidsForPort(port),
    port,
  };
}

export function assertDesktopBaselineClean(baseline) {
  assert.deepEqual(
    baseline.keeperPids,
    [],
    `Pre-existing keeper-ide.exe processes: ${baseline.keeperPids.join(", ")}`,
  );
  assert.deepEqual(
    baseline.listenerPids,
    [],
    `Pre-existing port ${baseline.port} listeners: ${baseline.listenerPids.join(", ")}`,
  );
}

function descendantsOf(processes, roots) {
  const descendants = new Set();
  let changed = true;
  while (changed) {
    changed = false;
    for (const entry of processes) {
      if (
        !roots.has(entry.pid)
        && !descendants.has(entry.pid)
        && (roots.has(entry.parentPid) || descendants.has(entry.parentPid))
      ) {
        descendants.add(entry.pid);
        changed = true;
      }
    }
  }
  return descendants;
}

function terminateWindowsPids(pids) {
  const alive = pids.filter(isProcessAlive);
  if (alive.length === 0) return;
  const result = spawnSync(
    "taskkill.exe",
    [...alive.flatMap((pid) => ["/PID", String(pid)]), "/T", "/F"],
    { encoding: "utf8", windowsHide: true },
  );
  const survivors = alive.filter(isProcessAlive);
  if (result.status !== 0 && survivors.length > 0) {
    throw new Error(
      `taskkill failed for PIDs ${survivors.join(",")} with code ${result.status}: `
      + `${result.stderr || result.stdout}`,
    );
  }
}

export class ProcessSupervisor {
  constructor({ rootPid, baseline, enforceDesktopClean = true }) {
    this.baseline = baseline;
    this.enforceDesktopClean = enforceDesktopClean;
    this.knownPids = new Set();
    this.lastCaptureAt = 0;
    this.port = baseline?.port ?? 1420;
    if (rootPid) this.knownPids.add(rootPid);
  }

  trackPid(pid) {
    if (Number.isInteger(pid) && pid > 0) this.knownPids.add(pid);
  }

  hasTrackedPid(pid) {
    return this.knownPids.has(pid);
  }

  async capture(force = false) {
    if (process.platform !== "win32") return;
    if (!force && performance.now() - this.lastCaptureAt < 500) return;
    const processes = snapshotWindowsProcesses();
    for (const pid of descendantsOf(processes, this.knownPids)) {
      this.knownPids.add(pid);
    }
    if (this.enforceDesktopClean) {
      for (const pid of keeperProcessPids(processes)) this.knownPids.add(pid);
      for (const pid of listenerPidsForPort(this.port)) this.knownPids.add(pid);
    }
    this.lastCaptureAt = performance.now();
  }

  async cleanup() {
    let lastError = null;

    for (let attempt = 0; attempt < CLEANUP_ATTEMPTS; attempt += 1) {
      await this.capture(true);
      const alive = [...this.knownPids].filter(isProcessAlive);
      if (alive.length === 0) return;
      try {
        if (process.platform === "win32") {
          terminateWindowsPids(alive.reverse());
        } else {
          for (const pid of alive.reverse()) {
            process.kill(pid, "SIGKILL");
          }
        }
      } catch (error) {
        lastError = error;
      }
      await delay(POLL_INTERVAL_MS);
    }

    const survivingKnown = [...this.knownPids].filter(isProcessAlive);
    const detail = lastError instanceof Error ? ` ${lastError.message}` : "";
    throw new Error(
      `Desktop cleanup left known=[${survivingKnown}] after `
      + `${CLEANUP_ATTEMPTS} attempts.${detail}`,
    );
  }
}

export async function waitForJsonFile(
  file,
  { timeoutMs, validate, processHandle, label, onPoll },
) {
  const startedAt = performance.now();
  let lastError = null;

  while (performance.now() - startedAt < timeoutMs) {
    if (onPoll) await onPoll();
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

  const detail = lastError instanceof Error
    ? ` Last observation: ${lastError.message}`
    : "";
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
  return { child, output: () => ({ stderr, stdout }) };
}

export async function waitForOutputMatch(processRun, pattern, timeoutMs, onPoll) {
  const deadline = performance.now() + timeoutMs;
  while (performance.now() < deadline) {
    if (onPoll) await onPoll();
    const match = processRun.output().stdout.match(pattern);
    if (match) return match;
    if (processRun.child.exitCode !== null) {
      throw new Error("Process exited before emitting the expected output.");
    }
    await delay(POLL_INTERVAL_MS);
  }
  throw new Error(`Timed out waiting for process output matching ${pattern}.`);
}

export async function observeProcessSurvival(pid, milliseconds = 500) {
  const deadline = performance.now() + milliseconds;
  while (performance.now() < deadline) {
    assert.equal(isProcessAlive(pid), true, `PID ${pid} exited after readiness.`);
    await delay(POLL_INTERVAL_MS);
  }
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

export async function runSupervisedCommand(
  command,
  args,
  { timeoutMs, enforceDesktopClean = true, ...options },
) {
  const baseline = captureDesktopBaseline();
  if (enforceDesktopClean) assertDesktopBaselineClean(baseline);
  const processRun = startCaptured(command, args, options);
  const supervisor = new ProcessSupervisor({
    baseline,
    enforceDesktopClean,
    rootPid: processRun.child.pid,
  });
  let timer;
  let timedOut = false;
  const captureTimer = setInterval(() => {
    void supervisor.capture();
  }, 500);
  try {
    const result = await Promise.race([
      new Promise((resolve, reject) => {
        processRun.child.once("error", reject);
        processRun.child.once("exit", (status, signal) => {
          resolve({ signal, status });
        });
      }),
      new Promise((resolve) => {
        timer = setTimeout(() => {
          timedOut = true;
          resolve({ signal: "TIMEOUT", status: null });
        }, timeoutMs);
      }),
    ]);
    return { ...result, ...processRun.output(), timedOut };
  } finally {
    clearInterval(captureTimer);
    clearTimeout(timer);
    await supervisor.cleanup();
  }
}
