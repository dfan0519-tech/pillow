import assert from "node:assert/strict";
import path from "node:path";

export function validateLifecycleEvidence(value, expected) {
  assert.equal(value.schemaVersion, 2);
  assert.equal(value.nonce, expected.nonce);
  assert.ok(Number.isInteger(value.pid) && value.pid > 0);
  assert.ok(value.startedAt >= expected.launchLowerBound);
  assert.ok(value.startedAt <= Date.now());
  assert.equal(path.resolve(value.appDataDir), path.resolve(expected.appDataDir));
  assert.equal(
    path.resolve(value.databasePath),
    path.join(path.resolve(expected.appDataDir), "keeper.db"),
  );
  assert.equal(
    path.resolve(value.webviewProfileDir),
    path.resolve(expected.webviewProfileDir),
  );
}

export function validateReadinessEvidence(value, expected) {
  validateLifecycleEvidence(value, expected);
  assert.equal(value.status, "ready");
  assert.ok(value.readyAtUnixMs >= value.startedAt);
}
