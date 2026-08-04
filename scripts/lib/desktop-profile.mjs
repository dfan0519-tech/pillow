import assert from "node:assert/strict";
import { readdir, stat } from "node:fs/promises";
import path from "node:path";

export function defaultWebViewProfilePath() {
  const localAppData = process.env.LOCALAPPDATA;
  if (!localAppData) throw new Error("LOCALAPPDATA is required on Windows.");
  return path.join(localAppData, "com.pillow.desktop", "EBWebView");
}

export async function snapshotPath(root) {
  const entries = [];
  async function visit(directory, relativeDirectory) {
    let children;
    try {
      children = await readdir(directory, { withFileTypes: true });
    } catch (error) {
      if (error?.code === "ENOENT") return;
      throw error;
    }
    children.sort((left, right) => left.name.localeCompare(right.name));
    for (const child of children) {
      const absolute = path.join(directory, child.name);
      const relative = path.join(relativeDirectory, child.name);
      const metadata = await stat(absolute);
      entries.push({
        kind: child.isDirectory() ? "directory" : "file",
        mtimeMs: Math.trunc(metadata.mtimeMs),
        relative,
        size: metadata.size,
      });
      if (child.isDirectory()) await visit(absolute, relative);
    }
  }
  await visit(root, "");
  return entries;
}

export async function assertPathUnchangedSince({ root, before, label }) {
  assert.ok(
    Array.isArray(before),
    "before snapshot must be an array returned by snapshotPath.",
  );
  const after = await snapshotPath(root);
  assert.deepEqual(after, before, `${label} changed during isolated acceptance.`);
  return after;
}

export function assertPathSnapshotUnchanged(before, after, label) {
  assert.deepEqual(after, before, `${label} changed during isolated acceptance.`);
}
