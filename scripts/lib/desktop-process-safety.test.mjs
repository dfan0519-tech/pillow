import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { ProcessSupervisor } from "./desktop-process.mjs";

function processEntry({
  pid,
  parentPid = 0,
  creationTicks = "133670000000000000",
  executablePath = "D:\\tools\\pillow.exe",
  name = "pillow.exe",
}) {
  return { creationTicks, executablePath, name, parentPid, pid };
}

function fakeChild(pid, onKill = () => {}) {
  return {
    exitCode: null,
    killCalls: 0,
    pid,
    kill() {
      this.killCalls += 1;
      onKill();
      return true;
    },
  };
}

function syntheticSupervisor({
  allowMsvcVctipTelemetry = false,
  child,
  snapshot,
  cleanupAttempts = 3,
  desktopObservation,
  enforceDesktopClean = false,
  platform = "win32",
  systemRoot = "D:\\Windows",
  windir = "",
}) {
  return new ProcessSupervisor({
    allowMsvcVctipTelemetry,
    baseline: { port: 1420 },
    child,
    cleanupAttempts,
    delayFn: async () => {},
    desktopObservation,
    enforceDesktopClean,
    platform,
    snapshot,
    systemRoot,
    windir,
  });
}

test("C2/C3 tracked-PID filtering uses the supervisor's sole identity store", () => {
  const root = processEntry({ pid: 901, creationTicks: "133670000000000100" });
  const child = processEntry({
    pid: 902,
    parentPid: 901,
    creationTicks: "133670000000000101",
  });
  const supervisor = syntheticSupervisor({
    child: fakeChild(901),
    snapshot: () => [root, child],
  });

  supervisor.capture(true);
  assert.deepEqual(supervisor.trackedPids(), [901, 902]);
  supervisor.retainTrackedPids((pid) => pid === 901);

  assert.deepEqual(supervisor.trackedPids(), [901]);
  assert.equal(supervisor.hasTrackedPid(902), false);
});

test("repeated tracking preserves the first immutable identity and permanently rejects reuse", async () => {
  let current = processEntry({ pid: 903, creationTicks: "133670000000000200" });
  const child = fakeChild(903);
  const supervisor = syntheticSupervisor({ child, snapshot: () => [current] });

  assert.equal(supervisor.trackPid(903), true);
  current = processEntry({ pid: 903, creationTicks: "133670000000000201" });
  assert.equal(supervisor.trackPid(903), false);

  await assert.rejects(supervisor.cleanup(), /identity mismatch.*PID 903/u);
  assert.equal(child.killCalls, 0);
});

test("natural exit is confirmed without a kill request and repeated cleanup stays successful", async () => {
  const identity = processEntry({ pid: 904 });
  let processes = [identity];
  const child = fakeChild(904);
  const supervisor = syntheticSupervisor({ child, snapshot: () => processes });
  processes = [];

  await supervisor.cleanup();
  await supervisor.cleanup();

  assert.equal(child.killCalls, 0);
  assert.equal(supervisor.trackedState(904), "confirmed-dead");
});

test("never-observed root identity fails closed", async () => {
  const child = fakeChild(905);
  const supervisor = syntheticSupervisor({ child, snapshot: () => [] });

  await assert.rejects(supervisor.cleanup(), /PID 905.*never observed/u);

  assert.equal(child.killCalls, 0);
});

test("a PID reused by svchost.exe is rejected without using the root handle", async () => {
  let current = processEntry({ pid: 906, creationTicks: "133670000000000300" });
  const child = fakeChild(906);
  const supervisor = syntheticSupervisor({ child, snapshot: () => [current] });
  current = processEntry({
    pid: 906,
    creationTicks: "133670000000000301",
    executablePath: "D:\\Windows\\System32\\svchost.exe",
    name: "svchost.exe",
  });

  await assert.rejects(supervisor.cleanup(), /identity mismatch.*PID 906/u);
  assert.equal(child.killCalls, 0);
});

test("a non-C Windows system root is denied even when identity matches", async () => {
  const identity = processEntry({
    pid: 907,
    executablePath: "D:\\Windows\\System32\\not-safe.exe",
  });
  const child = fakeChild(907);
  const supervisor = syntheticSupervisor({ child, snapshot: () => [identity] });

  await assert.rejects(supervisor.cleanup(), /Windows directory executable/u);
  assert.equal(child.killCalls, 0);
});

test("descendants require a later creation tick than their verified parent", async () => {
  const root = processEntry({ pid: 908, creationTicks: "133670000000000500" });
  const staleChild = processEntry({
    pid: 909,
    parentPid: 908,
    creationTicks: "133670000000000499",
  });
  let processes = [root, staleChild];
  const child = fakeChild(908, () => { processes = []; });
  const supervisor = syntheticSupervisor({ child, snapshot: () => processes });

  await supervisor.capture(true);
  assert.equal(supervisor.hasTrackedPid(909), false);
  await supervisor.cleanup();
  assert.equal(child.killCalls, 1);
});

test("only the launcher-owned root child handle is asked to terminate", async () => {
  const root = processEntry({ pid: 910, creationTicks: "133670000000000600" });
  const descendant = processEntry({
    pid: 911,
    parentPid: 910,
    creationTicks: "133670000000000601",
  });
  let processes = [root, descendant];
  const child = fakeChild(910, () => { processes = []; });
  const supervisor = syntheticSupervisor({ child, snapshot: () => processes });

  await supervisor.cleanup();

  assert.equal(child.killCalls, 1);
  assert.equal(supervisor.trackedState(910), "confirmed-dead");
  assert.equal(supervisor.trackedState(911), "confirmed-dead");
});

test("a kill request is not cleanup completion until fresh absence is confirmed", async () => {
  const root = processEntry({ pid: 912, creationTicks: "133670000000000700" });
  const child = fakeChild(912);
  const supervisor = syntheticSupervisor({
    child,
    cleanupAttempts: 2,
    snapshot: () => [root],
  });

  await assert.rejects(supervisor.cleanup(), /still present after root termination request/u);
  assert.equal(child.killCalls, 1);
  assert.equal(supervisor.trackedState(912), "terminating");
});

test("automatic cleanup contains no taskkill command or numeric termination signal", async () => {
  const source = await readFile(new URL("./desktop-process.mjs", import.meta.url), "utf8");

  assert.doesNotMatch(source, /taskkill\.exe/u);
  assert.doesNotMatch(source, /process\.kill\([^,]+,\s*["']SIG(?:KILL|TERM)/u);
});

test("startup [] -> [root] recovers the first identity only through the stable child handle", async () => {
  const root = processEntry({ pid: 920 });
  let processes = [];
  const child = fakeChild(920, () => { processes = []; });
  const supervisor = syntheticSupervisor({ child, snapshot: () => processes });

  processes = [root];
  await supervisor.capture(true);

  assert.equal(supervisor.trackedState(920), "observed");
  await supervisor.cleanup();
  assert.equal(child.killCalls, 1);
});

test("numeric-only descendants cannot recover a missing first identity", async () => {
  const root = processEntry({ pid: 921 });
  const descendant = processEntry({
    pid: 922,
    parentPid: 921,
    creationTicks: "133670000000000801",
  });
  let processes = [root];
  const supervisor = syntheticSupervisor({ child: fakeChild(921), snapshot: () => processes });

  assert.equal(supervisor.trackPid(922), false);
  processes = [root, descendant];
  await supervisor.capture(true);

  assert.equal(supervisor.hasTrackedPid(922), false);
  assert.equal(supervisor.trackedState(922), "never-observed");
});

test("the default cleanup observation budget remains at least 120 polls", () => {
  const supervisor = new ProcessSupervisor({
    baseline: { port: 1420 },
    child: fakeChild(923),
    delayFn: async () => {},
    enforceDesktopClean: false,
    platform: "win32",
    snapshot: () => [],
    systemRoot: "D:\\Windows",
  });

  assert.ok(supervisor.cleanupAttempts >= 120);
});

test("enforceDesktopClean rejects an unbound residual pillow/listener observation", async () => {
  const root = processEntry({ pid: 924 });
  let processes = [root];
  const child = fakeChild(924);
  const supervisor = syntheticSupervisor({
    child,
    desktopObservation: () => ({ keeperPids: [999], listenerPids: [998] }),
    enforceDesktopClean: true,
    snapshot: () => processes,
  });
  processes = [];

  await assert.rejects(supervisor.cleanup(), /unbound residual.*999.*998/u);
  assert.equal(child.killCalls, 0);
});

test("a present but incomplete root identity fails closed rather than becoming dead", async () => {
  const child = fakeChild(925);
  const supervisor = syntheticSupervisor({
    child,
    snapshot: () => [{ pid: 925, parentPid: 0, name: "pillow.exe" }],
  });

  await assert.rejects(supervisor.cleanup(), /incomplete identity.*PID 925/u);
  assert.equal(child.killCalls, 0);
  assert.equal(supervisor.trackedState(925), "identity-incomplete");
});

test("confirmed-dead is terminal when the numeric PID is later reused", async () => {
  const root = processEntry({ pid: 926 });
  let processes = [root];
  const child = fakeChild(926);
  const supervisor = syntheticSupervisor({ child, snapshot: () => processes });
  processes = [];

  await supervisor.cleanup();
  processes = [processEntry({
    pid: 926,
    creationTicks: "133670000000000900",
    executablePath: "D:\\Windows\\System32\\svchost.exe",
    name: "svchost.exe",
  })];
  await supervisor.cleanup();

  assert.equal(supervisor.trackedState(926), "confirmed-dead");
  assert.equal(child.killCalls, 0);
});

test("non-Windows cleanup uses only the fake child lifecycle and never snapshots", async () => {
  const child = fakeChild(927, () => { child.exitCode = 0; });
  const supervisor = syntheticSupervisor({
    child,
    platform: "linux",
    snapshot: () => { throw new Error("Windows snapshot must not run"); },
  });

  assert.equal(supervisor.trackPid(927), true);
  await supervisor.cleanup();

  assert.equal(child.killCalls, 1);
  assert.equal(supervisor.trackedState(927), "confirmed-dead");
});

test("Windows root termination refuses a missing SystemRoot/WINDIR", async () => {
  const root = processEntry({ pid: 928 });
  const child = fakeChild(928);
  const supervisor = syntheticSupervisor({
    child,
    snapshot: () => [root],
    systemRoot: "",
  });

  await assert.rejects(supervisor.cleanup(), /SystemRoot.*required/u);
  assert.equal(child.killCalls, 0);
});

test("enforced residual observations retry from nonempty to empty within budget", async () => {
  const root = processEntry({ pid: 930 });
  let processes = [root];
  let observations = 0;
  const supervisor = syntheticSupervisor({
    child: fakeChild(930),
    cleanupAttempts: 3,
    desktopObservation: () => {
      observations += 1;
      return observations === 1
        ? { keeperPids: [990], listenerPids: [] }
        : { keeperPids: [], listenerPids: [] };
    },
    enforceDesktopClean: true,
    snapshot: () => processes,
  });
  processes = [];

  await supervisor.cleanup();
  assert.equal(observations, 2);
});

test("enforced residual observations fail only after budget exhaustion", async () => {
  const root = processEntry({ pid: 931 });
  let processes = [root];
  let observations = 0;
  const supervisor = syntheticSupervisor({
    child: fakeChild(931),
    cleanupAttempts: 3,
    desktopObservation: () => {
      observations += 1;
      return { keeperPids: [991], listenerPids: [992] };
    },
    enforceDesktopClean: true,
    snapshot: () => processes,
  });
  processes = [];

  await assert.rejects(supervisor.cleanup(), /unbound residual.*991.*992/u);
  assert.equal(observations, 3);
});

test("empty SystemRoot falls back to a valid absolute WINDIR", async () => {
  const root = processEntry({ pid: 932, executablePath: "E:\\tools\\pillow.exe" });
  let processes = [root];
  const child = fakeChild(932, () => { processes = []; });
  const supervisor = syntheticSupervisor({
    child,
    snapshot: () => processes,
    systemRoot: "",
    windir: "E:\\Windows",
  });

  await supervisor.cleanup();
  assert.equal(child.killCalls, 1);
});

test("invalid and conflicting Windows root candidates refuse root termination", async () => {
  const root = processEntry({ pid: 933 });
  const invalidChild = fakeChild(933);
  const invalid = syntheticSupervisor({
    child: invalidChild,
    snapshot: () => [root],
    systemRoot: "relative\\windows",
    windir: "",
  });
  await assert.rejects(invalid.cleanup(), /invalid Windows root/u);
  assert.equal(invalidChild.killCalls, 0);

  const conflictChild = fakeChild(933);
  const conflict = syntheticSupervisor({
    child: conflictChild,
    snapshot: () => [root],
    systemRoot: "D:\\Windows",
    windir: "E:\\Windows",
  });
  await assert.rejects(conflict.cleanup(), /conflicting Windows roots/u);
  assert.equal(conflictChild.killCalls, 0);
});

test("a retained non-root never-observed PID blocks cleanup success", async () => {
  const root = processEntry({ pid: 934 });
  let processes = [root];
  const child = fakeChild(934);
  const supervisor = syntheticSupervisor({ child, snapshot: () => processes });
  assert.equal(supervisor.trackPid(935), false);
  processes = [];

  await assert.rejects(supervisor.cleanup(), /never-observed.*PID 935/u);
  assert.equal(child.killCalls, 0);
});

test("a verified parent records an incomplete snapshot child as permanent identity-incomplete", async () => {
  const root = processEntry({ pid: 936, creationTicks: "133670000000001000" });
  const incompleteChild = {
    pid: 937,
    parentPid: 936,
    creationTicks: "133670000000001001",
  };
  const child = fakeChild(936);
  const supervisor = syntheticSupervisor({
    child,
    snapshot: () => [root, incompleteChild],
  });

  await supervisor.capture(true);
  assert.equal(supervisor.trackedState(937), "identity-incomplete");
  await assert.rejects(supervisor.cleanup(), /incomplete identity.*PID 937/u);
  assert.equal(child.killCalls, 1);
});

test("immediate non-root PID reuse confirms only the original descendant dead without rebinding", async () => {
  const root = processEntry({ pid: 940, creationTicks: "133670000000002000" });
  const original = processEntry({
    pid: 941,
    parentPid: 940,
    creationTicks: "133670000000002001",
    name: "rustc.exe",
  });
  const replacement = processEntry({
    pid: 941,
    parentPid: 940,
    creationTicks: "133670000000002002",
    executablePath: "D:\\tools\\rustc-reused.exe",
    name: "rustc.exe",
  });
  let processes = [root, original];
  const child = fakeChild(940, () => { processes = []; });
  const supervisor = syntheticSupervisor({ child, snapshot: () => processes });
  await supervisor.capture(true);
  processes = [root, replacement];

  await supervisor.cleanup();

  assert.equal(supervisor.trackedState(941), "identity-replaced");
  assert.equal(child.killCalls, 1);
  assert.deepEqual(supervisor.trackedPids(), [940, 941]);
});

test("a complete descendant remains uncertain when incomplete observation precedes PID reuse without absence", async () => {
  const root = processEntry({ pid: 945, creationTicks: "133670000000002500" });
  const original = processEntry({
    pid: 946,
    parentPid: 945,
    creationTicks: "133670000000002501",
    name: "rustc.exe",
  });
  const incomplete = {
    pid: 946,
    parentPid: 945,
    creationTicks: "133670000000002501",
  };
  const replacement = processEntry({
    pid: 946,
    parentPid: 945,
    creationTicks: "133670000000002502",
    executablePath: "D:\\tools\\rustc-reused.exe",
    name: "rustc.exe",
  });
  let processes = [root, original];
  const child = fakeChild(945, () => { processes = [replacement]; });
  const supervisor = syntheticSupervisor({
    child,
    cleanupAttempts: 3,
    snapshot: () => processes,
  });
  await supervisor.capture(true);
  processes = [root, incomplete];

  await assert.rejects(supervisor.cleanup(), /incomplete identity.*PID 946/u);

  assert.equal(child.killCalls, 1);
  assert.equal(supervisor.trackedState(946), "identity-incomplete");
});

test("root PID reuse is confirmed dead only after its stable child handle reports exit", async () => {
  const original = processEntry({ pid: 942, creationTicks: "133670000000002100" });
  const replacement = processEntry({
    pid: 942,
    creationTicks: "133670000000002101",
    executablePath: "D:\\tools\\replacement.exe",
  });
  let processes = [original];
  const child = fakeChild(942);
  const supervisor = syntheticSupervisor({ child, snapshot: () => processes });
  child.exitCode = 0;
  processes = [replacement];

  await supervisor.cleanup();

  assert.equal(supervisor.trackedState(942), "confirmed-dead");
  assert.equal(child.killCalls, 0);
});

test("transient non-root incomplete identity is observed until absent while root handle may terminate", async () => {
  const root = processEntry({ pid: 950, creationTicks: "133670000000003000" });
  const incompleteChild = {
    pid: 951,
    parentPid: 950,
    creationTicks: "133670000000003001",
  };
  let processes = [root, incompleteChild];
  const child = fakeChild(950, () => { processes = []; });
  const supervisor = syntheticSupervisor({
    child,
    cleanupAttempts: 3,
    snapshot: () => processes,
  });
  await supervisor.capture(true);

  await supervisor.cleanup();

  assert.equal(child.killCalls, 1);
  assert.equal(supervisor.trackedState(951), "confirmed-dead");
});

test("persistent non-root incomplete identity fails only after its observation budget", async () => {
  const root = processEntry({ pid: 952, creationTicks: "133670000000003100" });
  const incompleteChild = {
    pid: 953,
    parentPid: 952,
    creationTicks: "133670000000003101",
  };
  let processes = [root, incompleteChild];
  const child = fakeChild(952, () => { processes = [incompleteChild]; });
  const supervisor = syntheticSupervisor({
    child,
    cleanupAttempts: 3,
    snapshot: () => processes,
  });
  await supervisor.capture(true);

  await assert.rejects(supervisor.cleanup(), /incomplete identity.*PID 953/u);
  assert.equal(child.killCalls, 1);
  assert.equal(supervisor.trackedState(953), "identity-incomplete");
});

const MSVC_VCTIP_PATH = "C:\\Program Files (x86)\\Microsoft Visual Studio\\2022\\BuildTools\\VC\\Tools\\MSVC\\14.44.35207\\bin\\HostX64\\x64\\vctip.exe";

function supervisorWithDetachedDescendant({
  descendant,
  allowMsvcVctipTelemetry = false,
}) {
  const root = processEntry({ pid: 960, creationTicks: "133670000000004000" });
  let processes = [root, descendant];
  const child = fakeChild(960, () => { processes = [descendant]; });
  const supervisor = syntheticSupervisor({
    allowMsvcVctipTelemetry,
    child,
    snapshot: () => processes,
  });
  return { child, supervisor };
}

test("detached exact VCTIP remains rejected unless the telemetry boundary is opted in", async () => {
  const vctip = processEntry({
    pid: 961,
    parentPid: 960,
    creationTicks: "133670000000004001",
    executablePath: MSVC_VCTIP_PATH,
    name: "VCTIP.EXE",
  });
  const { child, supervisor } = supervisorWithDetachedDescendant({ descendant: vctip });

  await assert.rejects(supervisor.cleanup(), /observation-only PIDs remain.*961/u);
  assert.equal(child.killCalls, 1);
});

test("opted-in boundary permits only detached exact MSVC VCTIP after root exit", async () => {
  const vctip = processEntry({
    pid: 962,
    parentPid: 960,
    creationTicks: "133670000000004002",
    executablePath: MSVC_VCTIP_PATH,
    name: "VCTIP.EXE",
  });
  const { child, supervisor } = supervisorWithDetachedDescendant({
    allowMsvcVctipTelemetry: true,
    descendant: vctip,
  });

  await supervisor.cleanup();
  assert.equal(child.killCalls, 1);
  assert.equal(supervisor.trackedState(962), "observed");
});

test("VCTIP telemetry boundary requires a strict true opt-in", async () => {
  const vctip = processEntry({
    pid: 968,
    parentPid: 960,
    creationTicks: "133670000000004008",
    executablePath: MSVC_VCTIP_PATH,
    name: "vctip.exe",
  });
  const { supervisor } = supervisorWithDetachedDescendant({
    allowMsvcVctipTelemetry: "true",
    descendant: vctip,
  });

  await assert.rejects(supervisor.cleanup(), /observation-only PIDs remain.*968/u);
});

test("opted-in boundary rejects VCTIP outside the MSVC installation shape", async () => {
  const vctip = processEntry({
    pid: 963,
    parentPid: 960,
    creationTicks: "133670000000004003",
    executablePath: "D:\\tools\\vctip.exe",
    name: "vctip.exe",
  });
  const { supervisor } = supervisorWithDetachedDescendant({
    allowMsvcVctipTelemetry: true,
    descendant: vctip,
  });

  await assert.rejects(supervisor.cleanup(), /observation-only PIDs remain.*963/u);
});

test("opted-in boundary rejects a Visual Studio lookalike outside Program Files", async () => {
  const vctip = processEntry({
    pid: 969,
    parentPid: 960,
    creationTicks: "133670000000004009",
    executablePath: "C:\\tmp\\Microsoft Visual Studio\\2022\\BuildTools\\VC\\Tools\\MSVC\\14.44.35207\\bin\\HostX64\\x64\\vctip.exe",
    name: "vctip.exe",
  });
  const { supervisor } = supervisorWithDetachedDescendant({
    allowMsvcVctipTelemetry: true,
    descendant: vctip,
  });

  await assert.rejects(supervisor.cleanup(), /observation-only PIDs remain.*969/u);
});

test("opted-in boundary never exempts a VCTIP root", async () => {
  const root = processEntry({
    pid: 964,
    creationTicks: "133670000000004004",
    executablePath: MSVC_VCTIP_PATH,
    name: "vctip.exe",
  });
  const child = fakeChild(964);
  const supervisor = syntheticSupervisor({
    allowMsvcVctipTelemetry: true,
    child,
    cleanupAttempts: 2,
    snapshot: () => [root],
  });

  await assert.rejects(supervisor.cleanup(), /root PID 964 still present/u);
  assert.equal(child.killCalls, 1);
});

test("opted-in boundary keeps Pillow, fake ACP, and WebView residuals fail-closed", async () => {
  for (const [pid, name, executablePath] of [
    [965, "pillow.exe", "D:\\tools\\pillow.exe"],
    [966, "fake_acp_agent.exe", "D:\\tools\\fake_acp_agent.exe"],
    [967, "msedgewebview2.exe", "C:\\Program Files (x86)\\Microsoft\\EdgeWebView\\Application\\msedgewebview2.exe"],
  ]) {
    const descendant = processEntry({
      pid,
      parentPid: 960,
      creationTicks: `133670000000004${pid}`,
      executablePath,
      name,
    });
    const { supervisor } = supervisorWithDetachedDescendant({
      allowMsvcVctipTelemetry: true,
      descendant,
    });

    await assert.rejects(supervisor.cleanup(), new RegExp(`observation-only PIDs remain.*${pid}`, "u"));
  }
});

test("C3 wrapper explicitly enables only MSVC VCTIP telemetry and outer desktop checks", async () => {
  const source = await readFile(
    new URL("../../apps/desktop/tests/e2e/c3-governed-write.spec.ts", import.meta.url),
    "utf8",
  );

  assert.match(source, /allowMsvcVctipTelemetry:\s*true/u);
  assert.match(source, /enforceDesktopClean:\s*true/u);
});

function incompleteDescendant({ creationTicks, parentPid, pid }) {
  return {
    creationTicks,
    executablePath: "",
    name: "unreadable.exe",
    parentPid,
    pid,
  };
}

test("an older incomplete candidate with a colliding parent PID is ignored", async () => {
  const root = processEntry({ pid: 970, creationTicks: "133670000000005000" });
  const incomplete = incompleteDescendant({
    pid: 971,
    parentPid: 970,
    creationTicks: "133670000000004999",
  });
  let processes = [root, incomplete];
  const supervisor = syntheticSupervisor({ child: fakeChild(970), snapshot: () => processes });

  await supervisor.capture();
  assert.equal(supervisor.trackedState(971), "untracked");
  processes = [];
  await supervisor.cleanup();
});

test("an equal-tick incomplete candidate with a colliding parent PID is ignored", async () => {
  const root = processEntry({ pid: 972, creationTicks: "133670000000005100" });
  const incomplete = incompleteDescendant({
    pid: 973,
    parentPid: 972,
    creationTicks: "133670000000005100",
  });
  let processes = [root, incomplete];
  const supervisor = syntheticSupervisor({ child: fakeChild(972), snapshot: () => processes });

  await supervisor.capture();
  assert.equal(supervisor.trackedState(973), "untracked");
  processes = [];
  await supervisor.cleanup();
});

test("a newer incomplete candidate remains tracked and blocks cleanup", async () => {
  const root = processEntry({ pid: 974, creationTicks: "133670000000005200" });
  const incomplete = incompleteDescendant({
    pid: 975,
    parentPid: 974,
    creationTicks: "133670000000005201",
  });
  let processes = [root, incomplete];
  const child = fakeChild(974, () => { processes = [incomplete]; });
  const supervisor = syntheticSupervisor({
    child,
    cleanupAttempts: 3,
    snapshot: () => processes,
  });

  await supervisor.capture();
  assert.equal(supervisor.trackedState(975), "identity-incomplete");
  await assert.rejects(supervisor.cleanup(), /incomplete identity.*PID 975/u);
  assert.equal(child.killCalls, 1);
});

test("a missing or invalid-tick incomplete candidate remains tracked and blocks cleanup", async () => {
  for (const [pid, creationTicks] of [[977, ""], [979, "not-a-tick"]]) {
    const root = processEntry({ pid: pid - 1, creationTicks: "133670000000005300" });
    const incomplete = incompleteDescendant({
      pid,
      parentPid: pid - 1,
      creationTicks,
    });
    let processes = [root, incomplete];
    const child = fakeChild(pid - 1, () => { processes = [incomplete]; });
    const supervisor = syntheticSupervisor({
      child,
      cleanupAttempts: 3,
      snapshot: () => processes,
    });

    await supervisor.capture();
    assert.equal(supervisor.trackedState(pid), "identity-incomplete");
    await assert.rejects(supervisor.cleanup(), new RegExp(`incomplete identity.*PID ${pid}`, "u"));
    assert.equal(child.killCalls, 1);
  }
});
