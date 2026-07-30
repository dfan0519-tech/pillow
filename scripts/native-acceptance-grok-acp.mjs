/**
 * Controlled Native Acceptance Harness for Grok ACP (Wave B2-H3).
 *
 * Default / dry-run: report SKIP (never PASS) when live opt-in is absent.
 * Live (controller only): baseline Grok/Keeper PIDs → ignored cargo test →
 * final PID snapshot → residual check → machine JSON + human report.
 *
 * Never prints profile contents, account identity, raw ACP envelopes, tokens,
 * or auth file data. Paths from env are treated as non-secret config references
 * and are not echoed into reports.
 */

import assert from "node:assert/strict";
import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

import {
  captureGrokKeeperBaseline,
  residualProcessIds,
  summarizeProcessSnapshot,
  classifyProcessIds,
} from "./lib/grok-process.mjs";
import {
  ENV,
  HARNESS_NAME,
  buildHarnessResult,
  buildSkipResult,
  readOptInConfig,
  renderHumanReport,
  validateAdapterEvidence,
} from "./lib/native-acceptance-grok-core.mjs";
import { assertNoSecrets, scanForSecrets } from "./lib/secret-scan.mjs";
import { runChecked } from "./lib/desktop-process.mjs";

const repositoryRoot = fileURLToPath(new URL("..", import.meta.url));

function parseArgs(argv) {
  const flags = new Set(argv.filter((a) => a.startsWith("--")));
  return {
    dryRun: flags.has("--dry-run") || flags.has("--dryrun"),
    live: flags.has("--live"),
    selfTest: flags.has("--self-test") || flags.has("--selftest"),
    help: flags.has("--help") || flags.has("-h"),
  };
}

function printHelp() {
  console.log(`Usage: node scripts/native-acceptance-grok-acp.mjs [options]

Options:
  --dry-run     Prove SKIP semantics without live Grok (default when not opted in)
  --live        Require full opt-in and run the ignored cargo live test
  --self-test   Syntax check, pure-logic asserts, dry-run SKIP, secret-scan
  --help        Show this help

Live opt-in (non-secret env):
  ${ENV.E2E}=1
  ${ENV.EXECUTABLE}=<path to grok binary>
  ${ENV.ISOLATED_HOME}=<isolated profile home directory>
  ${ENV.PROJECT_ROOT}=<project working directory>

Optional:
  ${ENV.RESULT}=<adapter evidence JSON path>
  ${ENV.OUT_DIR}=<directory for harness JSON + human report>

This harness never prints profile contents, tokens, raw ACP envelopes, or auth files.
`);
}

async function ensureOutDir(outDir) {
  await mkdir(outDir, { recursive: true });
  return outDir;
}

function defaultOutDir() {
  return path.join(repositoryRoot, "artifacts", "native-acceptance-grok-acp");
}

async function writeReports(outDir, result) {
  await ensureOutDir(outDir);
  const jsonPath = path.join(outDir, "result.json");
  const mdPath = path.join(outDir, "report.md");
  const jsonText = `${JSON.stringify(result, null, 2)}\n`;
  const mdText = renderHumanReport(result);
  assertNoSecrets(jsonText, "result.json");
  assertNoSecrets(mdText, "report.md");
  await writeFile(jsonPath, jsonText, "utf8");
  await writeFile(mdPath, mdText, "utf8");
  return { jsonPath, mdPath };
}

async function runDryRun({ outDir }) {
  const started = performance.now();
  const optIn = readOptInConfig(process.env);
  const reason = optIn.complete
    ? "Dry-run mode forced; live path not executed in this invocation."
    : `Live opt-in incomplete (missing: ${optIn.missing.join(", ") || "none"}). `
      + "Normal CI must SKIP, never PASS.";
  const result = buildSkipResult({
    mode: "dry-run",
    reason,
    elapsedMs: Math.round(performance.now() - started),
  });
  assert.equal(result.status, "skipped", "dry-run must report skipped, not passed");
  const paths = await writeReports(outDir, result);
  console.log(
    `KEEPER_GROK_NATIVE_ACCEPTANCE ${JSON.stringify({
      status: result.status,
      mode: result.mode,
      skipReason: result.skipReason,
      outDir,
      jsonPath: path.relative(repositoryRoot, paths.jsonPath),
      reportPath: path.relative(repositoryRoot, paths.mdPath),
    })}`,
  );
  return { result, exitCode: 0 };
}

async function runLive({ outDir }) {
  const started = performance.now();
  const optIn = readOptInConfig(process.env);
  if (!optIn.complete) {
    const result = buildSkipResult({
      mode: "live",
      reason:
        `Live mode requested but opt-in incomplete (missing: ${optIn.missing.join(", ")}). `
        + "Refusing to claim PASS.",
      elapsedMs: Math.round(performance.now() - started),
    });
    await writeReports(outDir, result);
    console.error(
      `Live opt-in incomplete. Set ${ENV.E2E}=1, ${ENV.EXECUTABLE}, `
        + `${ENV.ISOLATED_HOME}, ${ENV.PROJECT_ROOT}.`,
    );
    return { result, exitCode: 2 };
  }

  // Validate path existence without reading profile contents.
  for (const [label, p] of [
    ["executable", optIn._paths.executable],
    ["isolated home", optIn._paths.isolatedHome],
    ["project root", optIn._paths.projectRoot],
  ]) {
    try {
      await access(p);
    } catch {
      const result = buildHarnessResult({
        status: "failed",
        mode: "live",
        errorMessage: `Configured ${label} path is not accessible (path not printed).`,
        elapsedMs: Math.round(performance.now() - started),
      });
      await writeReports(outDir, result);
      return { result, exitCode: 1 };
    }
  }

  const evidencePath =
    optIn._paths.result
    || path.join(outDir, "adapter-evidence.json");
  await ensureOutDir(path.dirname(evidencePath));
  await ensureOutDir(outDir);

  const baseline = captureGrokKeeperBaseline();
  let cargoExitCode = null;
  let adapterResult = null;
  let errorMessage = null;

  const env = {
    ...process.env,
    [ENV.E2E]: "1",
    [ENV.EXECUTABLE]: optIn._paths.executable,
    [ENV.ISOLATED_HOME]: optIn._paths.isolatedHome,
    [ENV.PROJECT_ROOT]: optIn._paths.projectRoot,
    [ENV.RESULT]: evidencePath,
    // Never forward secrets intentionally; child test uses only these.
  };

  try {
    await runChecked(
      "cargo",
      [
        "test",
        "-p",
        "keeper-adapter-grok-acp",
        "--test",
        "live_native_acceptance",
        "--",
        "--ignored",
        "--nocapture",
        "controlled_live_native_acceptance",
      ],
      { cwd: repositoryRoot, env },
    );
    cargoExitCode = 0;
  } catch (error) {
    cargoExitCode = 1;
    errorMessage = error instanceof Error ? error.message : String(error);
    // Strip potential path dumps from cargo error noise for the report.
    errorMessage = redactHarnessError(errorMessage);
  }

  // The adapter's PID-bound shutdown proof completes before the secondary snapshot.
  const finalSnap = captureGrokKeeperBaseline();
  const residuals = residualProcessIds(baseline, finalSnap);

  if (cargoExitCode === 0) {
    try {
      const raw = await readFile(evidencePath, "utf8");
      assertNoSecrets(raw, "adapter-evidence.json");
      adapterResult = JSON.parse(raw);
      validateAdapterEvidence(adapterResult);
    } catch (error) {
      cargoExitCode = 1;
      errorMessage = error instanceof Error ? error.message : String(error);
    }
  }

  const ownedTreeClean = adapterResult?.ownedTreeClean === true;
  const residualOk = residuals.clean;
  const passed = cargoExitCode === 0 && ownedTreeClean && residualOk && adapterResult?.status === "passed";

  const checks = {
    optIn: { status: "passed" },
    probe: {
      status: adapterResult?.probe ? "passed" : "failed",
      adapterName: adapterResult?.adapterName ?? null,
      beforeSession: adapterResult?.probe?.beforeSession ?? null,
      afterSessionNew: adapterResult?.probe?.afterSessionNew ?? null,
      afterFirstSuccessfulPrompt:
        adapterResult?.probe?.afterFirstSuccessfulPrompt ?? null,
    },
    sessionNew: { status: adapterResult?.sessionNew === true ? "passed" : "failed" },
    readOnlyTurn: {
      status: adapterResult?.readOnlyTurn?.terminalCount === 1 ? "passed" : "failed",
      terminalKind: adapterResult?.readOnlyTurn?.terminalKind ?? null,
      terminalCount: adapterResult?.readOnlyTurn?.terminalCount ?? null,
    },
    cooperativeCancel: {
      status: adapterResult?.cooperativeCancel?.terminalCount === 1 ? "passed" : "failed",
      terminalKind: adapterResult?.cooperativeCancel?.terminalKind ?? null,
      terminalCount: adapterResult?.cooperativeCancel?.terminalCount ?? null,
    },
    shutdown: { status: adapterResult?.shutdownOk === true ? "passed" : "failed" },
    ownedTreeClean: {
      status: ownedTreeClean ? "passed" : "failed",
      ownedProviderRootPid: adapterResult?.ownedProviderRootPid ?? null,
    },
    residualPids: {
      status: residualOk ? "passed" : "failed",
      residualCount: residuals.residualCount,
    },
    singleTerminal: {
      status:
        adapterResult?.readOnlyTurn?.terminalCount === 1
        && adapterResult?.cooperativeCancel?.terminalCount === 1
          ? "passed"
          : "failed",
    },
    noWritePermission: {
      status: adapterResult?.writePermissionObserved === false ? "passed" : "failed",
    },
    adapterNameGrokAcp: {
      status: adapterResult?.adapterName === "grok-acp" ? "passed" : "failed",
    },
  };

  if (!residualOk && !errorMessage) {
    errorMessage = `Residual processes after live run: newGrok=[${residuals.newGrokPids}] newKeeper=[${residuals.newKeeperPids}]`;
  }

  const result = buildHarnessResult({
    status: passed ? "passed" : "failed",
    mode: "live",
    errorMessage,
    elapsedMs: Math.round(performance.now() - started),
    cargoExitCode,
    adapterResult: adapterResult
      ? {
          status: adapterResult.status,
          adapterName: adapterResult.adapterName,
          probe: {
            beforeSession: adapterResult.probe?.beforeSession ?? null,
            afterSessionNew: adapterResult.probe?.afterSessionNew ?? null,
            afterFirstSuccessfulPrompt:
              adapterResult.probe?.afterFirstSuccessfulPrompt ?? null,
          },
          readOnlyTerminal: adapterResult.readOnlyTurn?.terminalKind ?? null,
          cancelTerminal: adapterResult.cooperativeCancel?.terminalKind ?? null,
          shutdownOk: adapterResult.shutdownOk ?? null,
          ownedProviderRootPid: adapterResult.ownedProviderRootPid ?? null,
          ownedTreeClean: adapterResult.ownedTreeClean ?? null,
        }
      : null,
    checks,
    processBaseline: summarizeProcessSnapshot(baseline),
    processFinal: summarizeProcessSnapshot(finalSnap),
    residuals,
  });

  await writeReports(outDir, result);
  console.log(
    `KEEPER_GROK_NATIVE_ACCEPTANCE ${JSON.stringify({
      status: result.status,
      mode: result.mode,
      residualClean: residualOk,
      elapsedMs: result.elapsedMs,
    })}`,
  );
  return { result, exitCode: passed ? 0 : 1 };
}

function redactHarnessError(message) {
  let text = String(message);
  // Drop long path segments that might include usernames.
  text = text.replace(/[A-Za-z]:\\[^\s"'`]+/g, "<path>");
  text = text.replace(/\/home\/[^\s"'`]+/g, "<path>");
  text = text.replace(/\/Users\/[^\s"'`]+/g, "<path>");
  // Never keep possible envelope fragments.
  text = text.replace(/\{[^{}]*"jsonrpc"[^{}]*\}/g, "<redacted-json>");
  // Cap length.
  if (text.length > 800) text = `${text.slice(0, 800)}…`;
  return text;
}

async function runSelfTest() {
  const started = performance.now();
  const failures = [];

  // 1) Syntax check harness modules via node --check
  const modules = [
    "scripts/native-acceptance-grok-acp.mjs",
    "scripts/lib/native-acceptance-grok-core.mjs",
    "scripts/lib/grok-process.mjs",
    "scripts/lib/secret-scan.mjs",
  ];
  for (const rel of modules) {
    const full = path.join(repositoryRoot, rel);
    const check = spawnSync(process.execPath, ["--check", full], {
      encoding: "utf8",
      windowsHide: true,
    });
    if (check.status !== 0) {
      failures.push(`syntax ${rel}: ${check.stderr || check.stdout}`);
    }
  }

  // 2) Pure logic: incomplete opt-in is not complete
  {
    const cfg = readOptInConfig({});
    assert.equal(cfg.complete, false);
    assert.ok(cfg.missing.includes(ENV.E2E));
  }
  {
    const cfg = readOptInConfig({
      [ENV.E2E]: "1",
      [ENV.EXECUTABLE]: "C:\\tools\\grok.exe",
      [ENV.ISOLATED_HOME]: "C:\\tmp\\iso",
      [ENV.PROJECT_ROOT]: "C:\\tmp\\proj",
    });
    assert.equal(cfg.complete, true);
    // Paths must not appear in public skip/build results.
    const skip = buildSkipResult({ mode: "dry-run" });
    const dump = JSON.stringify(skip);
    assert.equal(skip.status, "skipped");
    assert.ok(!dump.includes("C:\\\\tools\\\\grok.exe"));
    assertNoSecrets(dump, "skip-result");
  }

  // 3) Residual PID math
  {
    const baseline = { grokPids: [10, 20], keeperPids: [1] };
    const finalSnap = { grokPids: [10, 20, 30], keeperPids: [1] };
    const r = residualProcessIds(baseline, finalSnap);
    assert.equal(r.clean, false);
    assert.deepEqual(r.newGrokPids, [30]);
    assert.deepEqual(r.newKeeperPids, []);
    const clean = residualProcessIds(baseline, { grokPids: [10, 20], keeperPids: [1] });
    assert.equal(clean.clean, true);
  }

  // 4) classifyProcessIds
  {
    const c = classifyProcessIds([
      { pid: 5, name: "grok.exe" },
      { pid: 6, name: "keeper-ide.exe" },
      { pid: 7, name: "chrome.exe" },
    ]);
    assert.deepEqual(c.grokPids, [5]);
    assert.deepEqual(c.keeperPids, [6]);
  }

  // 5) Secret scan catches forbidden patterns
  {
    const bad = scanForSecrets('{"jsonrpc":"2.0","method":"initialize"}');
    assert.equal(bad.clean, false);
    assert.ok(bad.findings.some((f) => f.id === "raw_acp_envelope"));
    const good = scanForSecrets('{"status":"skipped","harness":"native-acceptance-grok-acp"}');
    assert.equal(good.clean, true);
  }

  // 6) Dry-run SKIP end-to-end
  const tmpOut = path.join(os.tmpdir(), `keeper-h3-selftest-${Date.now()}`);
  const dry = await runDryRun({ outDir: tmpOut });
  assert.equal(dry.result.status, "skipped");
  assert.notEqual(dry.result.status, "passed");
  const dryJson = await readFile(path.join(tmpOut, "result.json"), "utf8");
  assertNoSecrets(dryJson, "self-test dry-run result");
  const parsed = JSON.parse(dryJson);
  assert.equal(parsed.status, "skipped");
  assert.equal(parsed.harness, HARNESS_NAME);

  // 7) Human report template renders without secrets
  const report = renderHumanReport(parsed);
  assert.match(report, /SKIP/);
  assertNoSecrets(report, "self-test human report");

  // 8) Adapter evidence validator rejects fake name
  {
    let rejected = false;
    try {
      validateAdapterEvidence({
        schemaVersion: 1,
        status: "passed",
        adapterName: "fake",
        probe: {
          beforeSession: { healthy: false, detail: "unavailable" },
          afterSessionNew: {
            healthy: false,
            detail: "transport_ready_auth_unverified",
          },
          afterFirstSuccessfulPrompt: { healthy: true, detail: "healthy" },
        },
        sessionNew: true,
        readOnlyTurn: { terminalKind: "turn_completed", terminalCount: 1 },
        cooperativeCancel: { terminalKind: "run_cancelled", terminalCount: 1 },
        writePermissionObserved: false,
        shutdownOk: true,
        ownedProviderRootPid: 42,
        ownedTreeClean: true,
      });
    } catch {
      rejected = true;
    }
    assert.equal(rejected, true, "must reject adapter_name=fake for live gate");
  }

  // 9) Adapter evidence validator accepts the factual H1 probe ladder.
  {
    assert.equal(
      validateAdapterEvidence({
        schemaVersion: 1,
        status: "passed",
        adapterName: "grok-acp",
        probe: {
          beforeSession: { healthy: false, detail: "unavailable" },
          afterSessionNew: {
            healthy: false,
            detail: "transport_ready_auth_unverified",
          },
          afterFirstSuccessfulPrompt: { healthy: true, detail: "healthy" },
        },
        sessionNew: true,
        readOnlyTurn: { terminalKind: "turn_completed", terminalCount: 1 },
        cooperativeCancel: { terminalKind: "run_cancelled", terminalCount: 1 },
        writePermissionObserved: false,
        shutdownOk: true,
        ownedProviderRootPid: 42,
        ownedTreeClean: true,
      }),
      true,
    );
  }

  // 10) Live ignored test is present and marked ignore in source
  {
    const liveSrc = await readFile(
      path.join(
        repositoryRoot,
        "crates/adapter-grok-acp/tests/live_native_acceptance.rs",
      ),
      "utf8",
    );
    assert.match(liveSrc, /#\[ignore/);
    assert.match(liveSrc, /controlled_live_native_acceptance/);
    assert.match(liveSrc, /KEEPER_GROK_ACP_E2E/);
  }

  if (failures.length > 0) {
    throw new Error(`self-test failures:\n${failures.join("\n")}`);
  }

  const summary = {
    status: "passed",
    mode: "self-test",
    elapsedMs: Math.round(performance.now() - started),
    checks: modules.length + 8,
  };
  console.log(`KEEPER_GROK_NATIVE_ACCEPTANCE_SELF_TEST ${JSON.stringify(summary)}`);
  return { result: summary, exitCode: 0 };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
    process.exit(0);
  }

  const outDir = process.env[ENV.OUT_DIR]
    ? path.resolve(process.env[ENV.OUT_DIR])
    : defaultOutDir();

  if (args.selfTest) {
    const { exitCode } = await runSelfTest();
    process.exit(exitCode);
  }

  if (args.live) {
    const { exitCode } = await runLive({ outDir });
    process.exit(exitCode);
  }

  // Default and --dry-run: always SKIP path (never PASS without controlled live).
  const { exitCode } = await runDryRun({ outDir });
  process.exit(exitCode);
}

main().catch((error) => {
  const message = error instanceof Error ? error.message : String(error);
  console.error(redactHarnessError(message));
  process.exit(1);
});
