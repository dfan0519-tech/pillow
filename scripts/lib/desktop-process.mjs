import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { readFile } from "node:fs/promises";

const POLL_INTERVAL_MS = 50;
const CLEANUP_ATTEMPTS = 120;
const SYSTEM_PROCESS_NAMES = new Set([
  "csrss.exe",
  "dwm.exe",
  "fontdrvhost.exe",
  "lsass.exe",
  "memory compression",
  "registry",
  "services.exe",
  "smss.exe",
  "svchost.exe",
  "system",
  "system idle process",
  "wininit.exe",
  "winlogon.exe",
]);
const MSVC_VCTIP_TELEMETRY_PATH = /^[a-z]:\\program files(?: \(x86\))?\\microsoft visual studio\\\d{4}\\[^\\]+\\vc\\tools\\msvc\\[^\\]+\\bin\\hostx64\\x64\\vctip\.exe$/u;

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

function normalizeProcessName(name) {
  return String(name ?? "").trim().toLowerCase();
}

function normalizeExecutablePath(executablePath) {
  return String(executablePath ?? "")
    .trim()
    .replaceAll("/", "\\")
    .replace(/\\+$/u, "")
    .toLowerCase();
}

function normalizeCreationTicks(creationTicks) {
  const text = String(creationTicks ?? "").trim();
  if (!/^\d+$/u.test(text)) return "";
  return BigInt(text).toString();
}

function processIdentity(entry) {
  const pid = Number(entry?.pid);
  const creationTicks = normalizeCreationTicks(entry?.creationTicks);
  const executablePath = normalizeExecutablePath(entry?.executablePath);
  const name = normalizeProcessName(entry?.name);
  if (!Number.isInteger(pid) || pid <= 0 || !creationTicks || !executablePath || !name) {
    return null;
  }
  return Object.freeze({ creationTicks, executablePath, name, pid });
}

function lookupProcess(processes, pid) {
  const entry = processes.find((candidate) => Number(candidate.pid) === pid);
  if (entry === undefined) return { identity: null, state: "absent" };
  const identity = processIdentity(entry);
  return identity === null
    ? { identity: null, state: "present-incomplete" }
    : { identity, state: "present-complete" };
}

function childHasExited(child) {
  return (child?.exitCode !== null && child?.exitCode !== undefined)
    || (child?.signalCode !== null && child?.signalCode !== undefined);
}

function resolveWindowsRoot(systemRoot, windir) {
  const candidates = [
    ["SystemRoot", systemRoot],
    ["WINDIR", windir],
  ].filter(([, value]) => String(value ?? "").trim());
  const roots = [];
  for (const [name, value] of candidates) {
    const normalized = normalizeExecutablePath(value);
    if (!/^[a-z]:\\/u.test(normalized) && !/^\\\\[^\\]+\\[^\\]+/u.test(normalized)) {
      return { error: `invalid Windows root ${name}`, root: null };
    }
    roots.push(normalized);
  }
  if (roots.length === 0) return { error: "SystemRoot/WINDIR is required", root: null };
  if (roots.length === 2 && roots[0] !== roots[1]) {
    return { error: "conflicting Windows roots", root: null };
  }
  return { error: null, root: roots[0] };
}

function identitiesMatch(left, right) {
  return left !== null
    && right !== null
    && left.pid === right.pid
    && left.creationTicks === right.creationTicks
    && left.executablePath === right.executablePath
    && left.name === right.name;
}

function systemProcessReason(identity, systemRoot) {
  if (SYSTEM_PROCESS_NAMES.has(identity.name)) return "system process name";
  const normalizedSystemRoot = normalizeExecutablePath(systemRoot);
  if (
    normalizedSystemRoot
    && (
      identity.executablePath === normalizedSystemRoot
      || identity.executablePath.startsWith(`${normalizedSystemRoot}\\`)
    )
  ) {
    return "Windows directory executable";
  }
  return null;
}

function isMsvcVctipTelemetryIdentity(identity) {
  return identity !== null
    && identity.name === "vctip.exe"
    && MSVC_VCTIP_TELEMETRY_PATH.test(identity.executablePath);
}

export function snapshotWindowsProcesses() {
  if (process.platform !== "win32") return [];
  const command = [
    "Get-CimInstance Win32_Process |",
    "ForEach-Object { [PSCustomObject]@{",
    "pid=[int]$_.ProcessId;",
    "parentPid=[int]$_.ParentProcessId;",
    "name=[string]$_.Name;",
    "executablePath=[string]$_.ExecutablePath;",
    "creationTicks=if ($_.CreationDate) { [string]$_.CreationDate.ToFileTimeUtc() } else { '' }",
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
    creationTicks: entry.creationTicks ?? "",
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
    .filter((entry) => entry.name.toLowerCase() === "pillow.exe")
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
    `Pre-existing pillow.exe processes: ${baseline.keeperPids.join(", ")}`,
  );
  assert.deepEqual(
    baseline.listenerPids,
    [],
    `Pre-existing port ${baseline.port} listeners: ${baseline.listenerPids.join(", ")}`,
  );
}

/** True when no pillow.exe and no supervised port listeners remain. */
export function isDesktopProcessSnapshotClean(port = 1420) {
  if (process.platform !== "win32") return true;
  const keepers = keeperProcessPids(snapshotWindowsProcesses());
  const listeners = listenerPidsForPort(port);
  return keepers.length === 0 && listeners.length === 0;
}

/**
 * Poll a fresh Windows process snapshot until pillow.exe and the desktop
 * port listener are gone. Not a fixed sleep: returns as soon as the snapshot is clean.
 */
export async function waitForDesktopProcessSnapshotClean({
  port = 1420,
  timeoutMs = 30_000,
  label = "desktop process snapshot",
} = {}) {
  const deadline = performance.now() + timeoutMs;
  let lastKeepers = [];
  let lastListeners = [];
  while (performance.now() < deadline) {
    if (process.platform !== "win32") return;
    const processes = snapshotWindowsProcesses();
    lastKeepers = keeperProcessPids(processes);
    lastListeners = listenerPidsForPort(port);
    if (lastKeepers.length === 0 && lastListeners.length === 0) return;
    await delay(POLL_INTERVAL_MS);
  }
  throw new Error(
    `${label} still dirty after ${timeoutMs} ms: `
    + `pillow.exe=[${lastKeepers.join(",")}] listeners=[${lastListeners.join(",")}]`,
  );
}

export class ProcessSupervisor {
  constructor({
    allowMsvcVctipTelemetry = false,
    child,
    baseline,
    cleanupAttempts = CLEANUP_ATTEMPTS,
    desktopObservation,
    delayFn = delay,
    enforceDesktopClean = true,
    platform = process.platform,
    snapshot = snapshotWindowsProcesses,
    systemRoot = process.env.SystemRoot,
    windir = process.env.WINDIR,
  }) {
    this.allowMsvcVctipTelemetry = allowMsvcVctipTelemetry === true;
    this.baseline = baseline;
    this.child = child;
    this.cleanupAttempts = cleanupAttempts;
    this.desktopObservation = desktopObservation ?? (() => ({
      keeperPids: keeperProcessPids(this.snapshot()),
      listenerPids: listenerPidsForPort(baseline?.port ?? 1420),
    }));
    this.delayFn = delayFn;
    this.enforceDesktopClean = enforceDesktopClean;
    this.platform = platform;
    this.records = new Map();
    this.snapshot = snapshot;
    this.systemRoot = systemRoot;
    this.windir = windir;
    this.lastDesktopObservation = null;
    this.rootPid = Number(child?.pid);
    if (!Number.isInteger(this.rootPid) || this.rootPid <= 0) {
      throw new Error("ProcessSupervisor requires the launcher-owned child handle.");
    }
    if (this.platform === "win32") this.captureIdentity(this.rootPid, true);
    else {
      this.records.set(this.rootPid, {
        identity: null,
        isRoot: true,
        state: childHasExited(child) ? "confirmed-dead" : "observed",
      });
    }
  }

  currentIdentity(processes, pid) {
    return lookupProcess(processes, pid);
  }

  captureIdentity(pid, isRoot = false, processes = this.snapshot()) {
    const existing = this.records.get(pid);
    const current = this.currentIdentity(processes, pid);
    if (existing !== undefined) {
      if (
        existing.state === "confirmed-dead"
        || existing.state === "identity-mismatch"
        || existing.state === "identity-replaced"
      ) return false;
      if (existing.identity === null) {
        if (existing.state === "identity-incomplete" && current.state === "absent") {
          existing.state = "confirmed-dead";
          return false;
        }
        if (existing.isRoot && existing.state === "never-observed" && !childHasExited(this.child)) {
          if (current.state === "present-complete") {
            existing.identity = current.identity;
            existing.state = "observed";
            return true;
          }
          if (current.state === "present-incomplete") existing.state = "identity-incomplete";
        }
        return false;
      }
      if (current.state === "absent") {
        existing.state = "confirmed-dead";
        return false;
      }
      if (current.state === "present-incomplete") {
        existing.state = "identity-incomplete";
        return false;
      }
      if (!identitiesMatch(existing.identity, current.identity)) {
        if (!existing.isRoot && existing.state === "identity-incomplete") {
          return false;
        }
        existing.state = existing.isRoot
          ? (childHasExited(this.child) ? "confirmed-dead" : "identity-mismatch")
          : "identity-replaced";
        return false;
      }
      return true;
    }
    if (current.state !== "present-complete") {
      this.records.set(pid, {
        identity: null,
        isRoot,
        state: current.state === "present-incomplete" ? "identity-incomplete" : "never-observed",
      });
      return false;
    }
    this.records.set(pid, { identity: current.identity, isRoot, state: "observed" });
    return true;
  }

  trackPid(pid) {
    if (!Number.isInteger(pid) || pid <= 0) return false;
    if (this.platform !== "win32") return pid === this.rootPid && this.records.has(pid);
    return this.captureIdentity(pid, pid === this.rootPid);
  }

  hasTrackedPid(pid) {
    const record = this.records.get(pid);
    return record !== undefined && record.identity !== null;
  }

  trackedPids() {
    return [...this.records.keys()];
  }

  trackedState(pid) {
    return this.records.get(pid)?.state ?? "untracked";
  }

  retainTrackedPids(keep) {
    for (const pid of this.trackedPids()) {
      if (!keep(pid)) this.records.delete(pid);
    }
    return this.trackedPids();
  }

  permitsDetachedMsvcVctipTelemetry(record, root) {
    return this.allowMsvcVctipTelemetry
      && !record.isRoot
      && record.state === "observed"
      && isMsvcVctipTelemetryIdentity(record.identity)
      && (root?.state === "confirmed-dead" || root?.state === "identity-replaced");
  }

  refreshRecords(processes) {
    for (const [pid, record] of this.records) {
      if (
        record.state === "confirmed-dead"
        || record.state === "identity-mismatch"
        || record.state === "identity-replaced"
      ) continue;
      const current = this.currentIdentity(processes, pid);
      if (record.identity === null) {
        this.captureIdentity(pid, record.isRoot, processes);
      } else if (current.state === "absent") record.state = "confirmed-dead";
      else if (current.state === "present-incomplete") record.state = "identity-incomplete";
      else if (!identitiesMatch(record.identity, current.identity)) {
        if (!record.isRoot && record.state === "identity-incomplete") continue;
        record.state = record.isRoot
          ? (childHasExited(this.child) ? "confirmed-dead" : "identity-mismatch")
          : "identity-replaced";
      }
    }
  }

  discoverDescendants(processes) {
    const parents = new Map(
      [...this.records.entries()]
        .filter(([, record]) => record.identity !== null && record.state === "observed")
        .map(([pid, record]) => [pid, record.identity]),
    );
    let changed = true;
    while (changed) {
      changed = false;
      for (const entry of processes) {
        const identity = processIdentity(entry);
        const incompleteCreationTicks = normalizeCreationTicks(entry.creationTicks);
        const parent = parents.get(Number(entry.parentPid));
        const pid = Number(entry.pid);
        if (
          identity === null
          && parent !== undefined
          && Number.isInteger(pid)
          && pid > 0
          && !this.records.has(pid)
          && (
            !incompleteCreationTicks
            || BigInt(incompleteCreationTicks) > BigInt(parent.creationTicks)
          )
        ) {
          this.records.set(pid, { identity: null, isRoot: false, state: "identity-incomplete" });
          continue;
        }
        if (
          identity !== null
          && parent !== undefined
          && !this.records.has(identity.pid)
          && BigInt(identity.creationTicks) > BigInt(parent.creationTicks)
        ) {
          this.records.set(identity.pid, { identity, isRoot: false, state: "observed" });
          parents.set(identity.pid, identity);
          changed = true;
        }
      }
    }
  }

  async capture() {
    const processes = this.platform === "win32" ? this.snapshot() : [];
    if (this.platform === "win32") {
      this.refreshRecords(processes);
      this.discoverDescendants(processes);
    }
    return processes;
  }

  async cleanup() {
    if (this.platform !== "win32") {
      const root = this.records.get(this.rootPid);
      if (root?.state === "confirmed-dead") return;
      for (let attempt = 0; attempt < this.cleanupAttempts; attempt += 1) {
        if (childHasExited(this.child)) {
          root.state = "confirmed-dead";
          return;
        }
        if (root.state === "observed") root.state = "terminating";
        if (attempt === 0 && this.child.kill() !== true) {
          throw new Error(`Desktop cleanup refused: root handle PID ${this.rootPid} rejected termination.`);
        }
        await this.delayFn(POLL_INTERVAL_MS);
      }
      throw new Error(`Desktop cleanup refused: root PID ${this.rootPid} still present after root termination request.`);
    }
    for (let attempt = 0; attempt < this.cleanupAttempts; attempt += 1) {
      const processes = await this.capture(true);
      const root = this.records.get(this.rootPid);
      const mismatch = [...this.records.entries()]
        .find(([, record]) => record.state === "identity-mismatch");
      if (mismatch !== undefined) {
        throw new Error(`Desktop cleanup refused: identity mismatch for PID ${mismatch[0]}.`);
      }
      const rootIncomplete = [...this.records.entries()]
        .find(([, record]) => record.isRoot && record.state === "identity-incomplete");
      if (rootIncomplete !== undefined) {
        throw new Error(`Desktop cleanup refused: incomplete identity for PID ${rootIncomplete[0]}.`);
      }
      const incompleteDescendant = [...this.records.entries()]
        .find(([, record]) => !record.isRoot && record.state === "identity-incomplete");
      const neverObservedDescendant = [...this.records.entries()]
        .find(([, record]) => !record.isRoot && record.state === "never-observed");
      if (neverObservedDescendant !== undefined) {
        throw new Error(
          `Desktop cleanup refused: never-observed descendant PID ${neverObservedDescendant[0]}.`,
        );
      }
      const live = [...this.records.entries()]
        .filter(
          ([, record]) => (
            record.state !== "confirmed-dead"
            && record.state !== "identity-replaced"
            && record.identity !== null
            && !this.permitsDetachedMsvcVctipTelemetry(record, root)
          ),
        );
      if (live.length === 0 && root?.state !== "never-observed") {
        if (incompleteDescendant !== undefined) {
          await this.delayFn(POLL_INTERVAL_MS);
          continue;
        }
        if (!this.enforceDesktopClean) return;
        const residual = this.desktopObservation();
        this.lastDesktopObservation = residual;
        if (residual.keeperPids.length === 0 && residual.listenerPids.length === 0) return;
        await this.delayFn(POLL_INTERVAL_MS);
        continue;
      }
      const liveRoot = root?.state === "observed" && root.identity !== null;
      if (liveRoot) {
        const windowsRoot = resolveWindowsRoot(this.systemRoot, this.windir);
        if (windowsRoot.error !== null) {
          throw new Error(`Desktop cleanup refused: ${windowsRoot.error}.`);
        }
        const unsafeReason = systemProcessReason(root.identity, windowsRoot.root);
        if (unsafeReason !== null) {
          throw new Error(`Desktop cleanup refused: PID ${this.rootPid} is a ${unsafeReason}.`);
        }
        root.state = "terminating";
        if (this.child.kill() !== true) {
          throw new Error(`Desktop cleanup refused: root handle PID ${this.rootPid} rejected termination.`);
        }
      }
      await this.delayFn(POLL_INTERVAL_MS);
    }
    const root = this.records.get(this.rootPid);
    if (root?.state === "never-observed") {
      throw new Error(`Desktop cleanup refused: root PID ${this.rootPid} was never observed.`);
    }
    const persistentIncomplete = [...this.records.entries()]
      .find(([, record]) => !record.isRoot && record.state === "identity-incomplete");
    if (persistentIncomplete !== undefined) {
      throw new Error(`Desktop cleanup refused: incomplete identity for PID ${persistentIncomplete[0]}.`);
    }
    if (
      this.lastDesktopObservation !== null
      && (
        this.lastDesktopObservation.keeperPids.length > 0
        || this.lastDesktopObservation.listenerPids.length > 0
      )
    ) {
      throw new Error(
        `Desktop cleanup refused: unbound residual pillow=[${this.lastDesktopObservation.keeperPids}] `
        + `listener=[${this.lastDesktopObservation.listenerPids}].`,
      );
    }
    if (root?.state === "terminating") {
      throw new Error(
        `Desktop cleanup refused: root PID ${this.rootPid} still present after root termination request.`,
      );
    }
    const live = this.trackedPids().filter((pid) => {
      const state = this.trackedState(pid);
      return state !== "confirmed-dead" && state !== "identity-replaced";
    });
    throw new Error(`Desktop cleanup refused: observation-only PIDs remain [${live.join(",")}].`);
  }
}

/** Wait until a child process has exited (graceful path preferred). */
export async function waitForChildExit(child, timeoutMs = 60_000, label = "process") {
  if (child.exitCode !== null || child.signalCode !== null) {
    return { status: child.exitCode, signal: child.signalCode };
  }
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(new Error(`${label} did not exit within ${timeoutMs} ms.`));
    }, timeoutMs);
    child.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.once("exit", (status, signal) => {
      clearTimeout(timer);
      resolve({ status, signal });
    });
  });
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
  {
    timeoutMs,
    allowMsvcVctipTelemetry = false,
    enforceDesktopClean = true,
    ...options
  },
) {
  const baseline = captureDesktopBaseline();
  if (enforceDesktopClean) assertDesktopBaselineClean(baseline);
  const processRun = startCaptured(command, args, options);
  const supervisor = new ProcessSupervisor({
    baseline,
    child: processRun.child,
    allowMsvcVctipTelemetry,
    enforceDesktopClean,
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
