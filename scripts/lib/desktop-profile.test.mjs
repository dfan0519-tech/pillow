import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  assertPathUnchangedSince,
  snapshotPath,
} from "./desktop-profile.mjs";

async function withTemporaryDirectory(run) {
  const root = await mkdtemp(path.join(os.tmpdir(), "pillow-desktop-profile-"));
  try {
    await run(root);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

test("assertPathUnchangedSince accepts an unchanged path", async () => {
  await withTemporaryDirectory(async (root) => {
    await writeFile(path.join(root, "profile-state.txt"), "unchanged\n", "utf8");
    const before = await snapshotPath(root);

    await assert.doesNotReject(assertPathUnchangedSince({
      root,
      before,
      label: "temporary profile",
    }));
  });
});

test("assertPathUnchangedSince reports the caller label after a file changes", async () => {
  await withTemporaryDirectory(async (root) => {
    const file = path.join(root, "profile-state.txt");
    await writeFile(file, "before\n", "utf8");
    const before = await snapshotPath(root);
    await writeFile(file, "after with a different size\n", "utf8");

    await assert.rejects(
      assertPathUnchangedSince({ root, before, label: "caller profile label" }),
      /caller profile label changed during isolated acceptance/u,
    );
  });
});

test("assertPathUnchangedSince rejects a missing or invalid before snapshot", async () => {
  await withTemporaryDirectory(async (root) => {
    await assert.rejects(
      assertPathUnchangedSince({ root, label: "missing snapshot" }),
      /before snapshot must be an array returned by snapshotPath/u,
    );
    await assert.rejects(
      assertPathUnchangedSince({ root, before: { entries: [] }, label: "invalid snapshot" }),
      /before snapshot must be an array returned by snapshotPath/u,
    );
  });
});

test("C3 governed-write acceptance supplies root, before, and label by name", async () => {
  const source = await readFile(
    new URL("../wave-c3-governed-write-acceptance.mjs", import.meta.url),
    "utf8",
  );

  assert.match(source, /assertPathUnchangedSince/u);
  assert.match(
    source,
    /await assertPathUnchangedSince\(\{\s*root: realProfile,\s*before: realProfileBefore,\s*label: "real LocalAppData WebView2 profile",\s*\}\)/u,
  );
});
