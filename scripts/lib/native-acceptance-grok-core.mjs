/**
 * Pure logic for Grok ACP controlled native acceptance harness.
 * No process launch, no filesystem profile reads, no secret material.
 */

export const HARNESS_NAME = "native-acceptance-grok-acp";
export const HARNESS_SCHEMA_VERSION = 1;

/** Non-secret opt-in environment variable names. */
export const ENV = {
  E2E: "KEEPER_GROK_ACP_E2E",
  EXECUTABLE: "KEEPER_GROK_EXECUTABLE",
  ISOLATED_HOME: "KEEPER_GROK_ISOLATED_HOME",
  PROJECT_ROOT: "KEEPER_GROK_PROJECT_ROOT",
  RESULT: "KEEPER_GROK_ACCEPTANCE_RESULT",
  OUT_DIR: "KEEPER_GROK_ACCEPTANCE_OUT",
};

/** Read-only prompt: forbids tool mutation. */
export const READ_ONLY_PROMPT =
  "Reply with one short sentence confirming you received this message. "
  + "Do not use any tools. Do not write, edit, delete, create, move, or execute files. "
  + "Text response only.";

/** Longer prompt intended to allow cooperative cancel mid-stream. */
export const CANCEL_PROMPT =
  "Write a careful multi-paragraph explanation of why software tests must "
  + "distinguish SKIP from PASS. Use several sentences. Do not use any tools. "
  + "Do not write, edit, delete, create, move, or execute files. Text only.";

/**
 * @param {NodeJS.ProcessEnv | Record<string, string | undefined>} env
 */
export function readOptInConfig(env = process.env) {
  const e2eRaw = String(env[ENV.E2E] ?? "").trim();
  const e2eEnabled = e2eRaw === "1" || e2eRaw.toLowerCase() === "true";
  const executable = String(env[ENV.EXECUTABLE] ?? "").trim();
  const isolatedHome = String(env[ENV.ISOLATED_HOME] ?? "").trim();
  const projectRoot = String(env[ENV.PROJECT_ROOT] ?? "").trim();
  const resultPath = String(env[ENV.RESULT] ?? "").trim();
  const missing = [];
  if (!e2eEnabled) missing.push(ENV.E2E);
  if (!executable) missing.push(ENV.EXECUTABLE);
  if (!isolatedHome) missing.push(ENV.ISOLATED_HOME);
  if (!projectRoot) missing.push(ENV.PROJECT_ROOT);
  return {
    e2eEnabled,
    executableConfigured: executable.length > 0,
    isolatedHomeConfigured: isolatedHome.length > 0,
    projectRootConfigured: projectRoot.length > 0,
    resultPathConfigured: resultPath.length > 0,
    /** Never expose full path strings in public reports. */
    pathsPresent: {
      executable: executable.length > 0,
      isolatedHome: isolatedHome.length > 0,
      projectRoot: projectRoot.length > 0,
      result: resultPath.length > 0,
    },
    complete: missing.length === 0,
    missing,
    /** Internal only — callers must not print these into reports. */
    _paths: {
      executable,
      isolatedHome,
      projectRoot,
      result: resultPath,
    },
  };
}

/**
 * Build a machine-readable harness result. Callers must secret-scan before print.
 * @param {object} input
 */
export function buildHarnessResult(input) {
  const {
    status,
    mode,
    skipReason = null,
    errorMessage = null,
    checks = {},
    processBaseline = null,
    processFinal = null,
    residuals = null,
    elapsedMs = null,
    cargoExitCode = null,
    adapterResult = null,
  } = input;

  return {
    schemaVersion: HARNESS_SCHEMA_VERSION,
    harness: HARNESS_NAME,
    status,
    mode,
    platform: process.platform,
    timestamp: new Date().toISOString(),
    skipReason,
    errorMessage,
    elapsedMs,
    cargoExitCode,
    optIn: {
      requires: [ENV.E2E, ENV.EXECUTABLE, ENV.ISOLATED_HOME, ENV.PROJECT_ROOT],
      e2eFlag: ENV.E2E,
      note: "Paths are non-secret config references; contents are never printed.",
    },
    checks,
    process: {
      baseline: processBaseline,
      final: processFinal,
      residuals,
    },
    adapterResult,
    nonClaims: [
      "Dry-run / SKIP is not live Grok connectivity evidence.",
      "Fake adapter and fixture replay are not B2 live evidence.",
      "Harness refuses to print profile contents, account identity, raw ACP envelopes, tokens, or auth files.",
      "Controller must run the controlled live path with a real isolated profile.",
    ],
  };
}

export function buildSkipResult({ mode = "dry-run", reason, elapsedMs = 0 } = {}) {
  return buildHarnessResult({
    status: "skipped",
    mode,
    skipReason: reason
      ?? `Live opt-in incomplete. Set ${ENV.E2E}=1 plus ${ENV.EXECUTABLE}, `
        + `${ENV.ISOLATED_HOME}, and ${ENV.PROJECT_ROOT}.`,
    elapsedMs,
    checks: {
      optIn: { status: "skipped" },
      probe: { status: "skipped" },
      sessionNew: { status: "skipped" },
      readOnlyTurn: { status: "skipped" },
      cooperativeCancel: { status: "skipped" },
      shutdown: { status: "skipped" },
      ownedTreeClean: { status: "skipped" },
      residualPids: { status: "skipped" },
      singleTerminal: { status: "skipped" },
      noWritePermission: { status: "skipped" },
      adapterNameGrokAcp: { status: "skipped" },
    },
  });
}

/**
 * Validate adapter-produced evidence JSON (redacted fields only).
 * @param {unknown} value
 */
export function validateAdapterEvidence(value) {
  if (!value || typeof value !== "object") {
    throw new Error("adapter evidence must be an object");
  }
  const v = value;
  if (v.schemaVersion !== 1) {
    throw new Error(`unexpected adapter evidence schemaVersion: ${v.schemaVersion}`);
  }
  if (v.status !== "passed") {
    throw new Error(`adapter evidence status is ${v.status}: ${v.errorMessage ?? ""}`);
  }
  if (v.adapterName !== "grok-acp") {
    throw new Error(`adapterName must be grok-acp, got ${v.adapterName}`);
  }
  const probe = v.probe;
  if (
    probe?.beforeSession?.healthy !== false
    || probe.beforeSession.detail !== "unavailable"
  ) {
    throw new Error("probe.beforeSession must be unavailable and not healthy");
  }
  if (
    probe?.afterSessionNew?.healthy !== false
    || probe.afterSessionNew.detail !== "transport_ready_auth_unverified"
  ) {
    throw new Error(
      "probe.afterSessionNew must be transport_ready_auth_unverified and not healthy",
    );
  }
  if (
    probe?.afterFirstSuccessfulPrompt?.healthy !== true
    || probe.afterFirstSuccessfulPrompt.detail !== "healthy"
  ) {
    throw new Error(
      "probe.afterFirstSuccessfulPrompt must be healthy for live pass",
    );
  }
  for (const key of ["readOnlyTurn", "cooperativeCancel"]) {
    const turn = v[key];
    if (!turn || turn.terminalCount !== 1) {
      throw new Error(`${key}.terminalCount must be exactly 1`);
    }
    if (typeof turn.terminalKind !== "string" || turn.terminalKind.length === 0) {
      throw new Error(`${key}.terminalKind required`);
    }
  }
  if (v.readOnlyTurn.terminalKind !== "turn_completed") {
    throw new Error(
      `readOnlyTurn expected turn_completed, got ${v.readOnlyTurn.terminalKind}`,
    );
  }
  if (v.cooperativeCancel.terminalKind !== "run_cancelled") {
    throw new Error(
      `cooperativeCancel expected run_cancelled, got ${v.cooperativeCancel.terminalKind}`,
    );
  }
  if (v.writePermissionObserved === true) {
    throw new Error("write permission must not be allowed during live acceptance");
  }
  if (v.shutdownOk !== true) {
    throw new Error("shutdownOk must be true");
  }
  if (!Number.isInteger(v.ownedProviderRootPid) || v.ownedProviderRootPid <= 0) {
    throw new Error("ownedProviderRootPid must be a positive integer");
  }
  if (v.ownedTreeClean !== true) {
    throw new Error("ownedTreeClean must be true");
  }
  return true;
}

/**
 * Render a human-readable report template (Markdown).
 * @param {object} result harness JSON
 */
export function renderHumanReport(result) {
  const lines = [
    "# Wave B2-H3 Controlled Native Acceptance Report",
    "",
    `Generated: ${result.timestamp ?? "(unknown)"}`,
    `Harness: \`${result.harness}\` schema ${result.schemaVersion}`,
    `Mode: **${result.mode}**`,
    `Status: **${String(result.status).toUpperCase()}**`,
    `Platform: ${result.platform}`,
    "",
    "## Opt-in",
    "",
    "Live execution requires non-secret environment variables:",
    "",
    `- \`${ENV.E2E}=1\``,
    `- \`${ENV.EXECUTABLE}\` — path to Grok CLI (not printed in reports)`,
    `- \`${ENV.ISOLATED_HOME}\` — isolated profile home (contents never read or printed)`,
    `- \`${ENV.PROJECT_ROOT}\` — project working directory for session/new`,
    "",
    "Normal CI and `cargo test --workspace` **SKIP** the live test; they must never claim PASS.",
    "",
  ];

  if (result.status === "skipped") {
    lines.push(
      "## SKIP",
      "",
      result.skipReason ?? "Live opt-in not enabled.",
      "",
      "This is intentional. SKIP is not PASS.",
      "",
    );
  }

  if (result.errorMessage) {
    lines.push("## Error", "", result.errorMessage, "");
  }

  lines.push("## Checks", "");
  const checks = result.checks ?? {};
  for (const [name, check] of Object.entries(checks)) {
    const status = check?.status ?? "unknown";
    lines.push(`- **${name}**: ${status}`);
  }
  lines.push("");

  const residuals = result.process?.residuals;
  if (residuals) {
    lines.push(
      "## Process residuals",
      "",
      `- clean: ${residuals.clean}`,
      `- new Grok PIDs: ${(residuals.newGrokPids ?? []).join(", ") || "(none)"}`,
      `- new Keeper PIDs: ${(residuals.newKeeperPids ?? []).join(", ") || "(none)"}`,
      "",
    );
  }

  lines.push(
    "## Non-claims",
    "",
    ...(result.nonClaims ?? []).map((c) => `- ${c}`),
    "",
    "## Controller live run checklist",
    "",
    "1. Provision an isolated Grok profile home (do not use the interactive user profile).",
    "2. Set opt-in env vars; never commit them or paste auth file contents into tickets.",
    "3. Run `node scripts/native-acceptance-grok-acp.mjs --live` (or package script).",
    "4. Confirm machine JSON `status=passed`, residual PIDs clean, secret-scan clean.",
    "5. Attach redacted report only — no raw ACP envelopes, tokens, or profile dumps.",
    "",
  );

  return `${lines.join("\n")}\n`;
}
