/**
 * Windows process snapshots for Grok ACP native acceptance.
 * Tracks Grok CLI and Keeper desktop process IDs only — never dumps env,
 * command lines with secrets, or profile file contents.
 */

import { snapshotWindowsProcesses } from "./desktop-process.mjs";

const GROK_NAMES = new Set(["grok.exe", "grok"]);
const KEEPER_NAMES = new Set(["keeper-ide.exe", "keeper-ide"]);

function normalizeName(name) {
  return String(name ?? "").trim().toLowerCase();
}

/**
 * @param {Array<{ pid: number, name: string, executablePath?: string }>} processes
 * @returns {{ grokPids: number[], keeperPids: number[] }}
 */
export function classifyProcessIds(processes) {
  const grokPids = [];
  const keeperPids = [];
  for (const entry of processes ?? []) {
    const name = normalizeName(entry.name);
    const pid = Number(entry.pid);
    if (!Number.isInteger(pid) || pid <= 0) continue;
    if (GROK_NAMES.has(name)) grokPids.push(pid);
    if (KEEPER_NAMES.has(name)) keeperPids.push(pid);
  }
  grokPids.sort((a, b) => a - b);
  keeperPids.sort((a, b) => a - b);
  return { grokPids, keeperPids };
}

/**
 * Snapshot current Grok and Keeper process IDs (Windows). Empty elsewhere.
 * @returns {{ platform: string, grokPids: number[], keeperPids: number[], capturedAtUnixMs: number }}
 */
export function captureGrokKeeperBaseline() {
  const capturedAtUnixMs = Date.now();
  if (process.platform !== "win32") {
    return {
      platform: process.platform,
      grokPids: [],
      keeperPids: [],
      capturedAtUnixMs,
    };
  }
  const classified = classifyProcessIds(snapshotWindowsProcesses());
  return {
    platform: "win32",
    grokPids: classified.grokPids,
    keeperPids: classified.keeperPids,
    capturedAtUnixMs,
  };
}

/**
 * Compute PIDs present in `finalSnap` but not in `baseline`.
 * @param {{ grokPids: number[], keeperPids: number[] }} baseline
 * @param {{ grokPids: number[], keeperPids: number[] }} finalSnap
 */
export function residualProcessIds(baseline, finalSnap) {
  const baseGrok = new Set(baseline?.grokPids ?? []);
  const baseKeeper = new Set(baseline?.keeperPids ?? []);
  const newGrokPids = (finalSnap?.grokPids ?? []).filter((pid) => !baseGrok.has(pid));
  const newKeeperPids = (finalSnap?.keeperPids ?? []).filter((pid) => !baseKeeper.has(pid));
  return {
    newGrokPids,
    newKeeperPids,
    residualCount: newGrokPids.length + newKeeperPids.length,
    clean: newGrokPids.length === 0 && newKeeperPids.length === 0,
  };
}

/**
 * Safe summary for reports — PIDs only, no executable paths.
 */
export function summarizeProcessSnapshot(snap) {
  return {
    platform: snap?.platform ?? process.platform,
    grokPidCount: (snap?.grokPids ?? []).length,
    keeperPidCount: (snap?.keeperPids ?? []).length,
    grokPids: [...(snap?.grokPids ?? [])],
    keeperPids: [...(snap?.keeperPids ?? [])],
    capturedAtUnixMs: snap?.capturedAtUnixMs ?? null,
  };
}
