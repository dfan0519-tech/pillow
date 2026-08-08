/**
 * ACP protocol lock drift check against upstream grok-build Cargo.lock.
 *
 * Pillow's Grok ACP adapter is hand-written and does not depend on
 * `agent-client-protocol`. Upstream wire compatibility is therefore only
 * observable via the locked crate versions/checksums in upstream Cargo.lock.
 *
 * This script:
 * - reads the in-repo baseline (docs/references/acp-protocol-lock-baseline.json);
 * - fetches upstream Cargo.lock as plain text (no clone, no install, no build);
 * - compares name/version/source/checksum for each baseline crate;
 * - fails with a clear diff when locks diverge;
 * - on network/offline failure, exits 0 with an explicit 未核对 skip marker
 *   (never silent pass-as-verified; never hard-fails the gate for offline).
 *
 * Never executes upstream code. Never starts ACP sessions. Never reads auth.
 */

import { readFileSync, writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { request as httpsRequest } from "node:https";
import { request as httpRequest } from "node:http";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = join(__dirname, "..");
const defaultBaselinePath = join(
  repositoryRoot,
  "docs",
  "references",
  "acp-protocol-lock-baseline.json",
);

const SKIP_MARKER = "ACP_DRIFT_CHECK: SKIPPED (未核对)";
const OK_MARKER = "ACP_DRIFT_CHECK: OK";
const DRIFT_MARKER = "ACP_DRIFT_CHECK: DRIFT";
const DEFAULT_TIMEOUT_MS = 20_000;

/**
 * Parse a Cargo.lock (v3/v4 style) into package records keyed by name.
 * Only top-level `[[package]]` tables are considered; no code execution.
 * @param {string} text
 * @returns {Map<string, { name: string, version: string, source?: string, checksum?: string }>}
 */
export function parseCargoLockPackages(text) {
  const packages = new Map();
  const blocks = String(text).split(/\n(?=\[\[package\]\])/u);
  for (const block of blocks) {
    if (!/^\s*\[\[package\]\]/mu.test(block)) continue;
    const name = matchField(block, "name");
    const version = matchField(block, "version");
    if (!name || !version) continue;
    const source = matchField(block, "source") ?? undefined;
    const checksum = matchField(block, "checksum") ?? undefined;
    // Prefer registry entries with checksum when duplicates exist.
    const prev = packages.get(name);
    if (!prev || (checksum && !prev.checksum)) {
      packages.set(name, { name, version, source, checksum });
    }
  }
  return packages;
}

/**
 * @param {string} block
 * @param {string} field
 * @returns {string | null}
 */
function matchField(block, field) {
  const re = new RegExp(`^${field}\\s*=\\s*"([^"]*)"`, "mu");
  const m = block.match(re);
  return m ? m[1] : null;
}

/**
 * Compare baseline crate pins to parsed lock packages.
 * @param {Array<{ name: string, version: string, source?: string, checksum?: string }>} baselineCrates
 * @param {Map<string, { name: string, version: string, source?: string, checksum?: string }>} packages
 * @returns {{ ok: boolean, differences: string[] }}
 */
export function compareBaselineToPackages(baselineCrates, packages) {
  const differences = [];
  for (const expected of baselineCrates) {
    const actual = packages.get(expected.name);
    if (!actual) {
      differences.push(
        `missing package ${expected.name} (expected version=${expected.version} checksum=${expected.checksum})`,
      );
      continue;
    }
    if (actual.version !== expected.version) {
      differences.push(
        `${expected.name}: version ${expected.version} -> ${actual.version}`,
      );
    }
    if (expected.source && actual.source && actual.source !== expected.source) {
      differences.push(
        `${expected.name}: source ${expected.source} -> ${actual.source}`,
      );
    }
    if (expected.checksum) {
      if (!actual.checksum) {
        differences.push(
          `${expected.name}: checksum missing upstream (expected ${expected.checksum})`,
        );
      } else if (actual.checksum !== expected.checksum) {
        differences.push(
          `${expected.name}: checksum ${expected.checksum} -> ${actual.checksum}`,
        );
      }
    }
  }
  return { ok: differences.length === 0, differences };
}

/**
 * Build raw.githubusercontent.com URL for a lockfile.
 * @param {{ repository: string, lockfilePath: string, checkRef?: string, baselineCommit?: string }} source
 * @returns {string}
 */
export function lockfileUrlFromSource(source) {
  const repo = String(source.repository || "").replace(/\.git$/u, "");
  const m = repo.match(/github\.com[/:]([^/]+)\/([^/]+)/u);
  if (!m) {
    throw new Error(`unsupported repository URL for raw fetch: ${repo}`);
  }
  const owner = m[1];
  const name = m[2];
  const ref = source.checkRef || source.baselineCommit || "main";
  const path = source.lockfilePath || "Cargo.lock";
  return `https://raw.githubusercontent.com/${owner}/${name}/${ref}/${path}`;
}

/**
 * Fetch text with timeout via node:https/http (no undici).
 * Throws on non-OK HTTP or network errors. Follows one hop of redirects.
 * Intentionally does not execute any response body as code.
 * @param {string} url
 * @param {{ timeoutMs?: number, fetchImpl?: (url: string, init?: object) => Promise<{ ok: boolean, status: number, text: () => Promise<string> }>, redirectHops?: number }} [opts]
 * @returns {Promise<string>}
 */
export async function fetchText(url, opts = {}) {
  const timeoutMs = opts.timeoutMs ?? DEFAULT_TIMEOUT_MS;
  if (typeof opts.fetchImpl === "function") {
    const response = await opts.fetchImpl(url, {
      headers: { Accept: "text/plain" },
    });
    if (!response.ok) {
      throw new Error(`HTTP ${response.status} for ${url}`);
    }
    return await response.text();
  }
  return await httpGetText(url, timeoutMs, opts.redirectHops ?? 5);
}

/**
 * @param {string} url
 * @param {number} timeoutMs
 * @param {number} redirectHops
 * @returns {Promise<string>}
 */
function httpGetText(url, timeoutMs, redirectHops) {
  return new Promise((resolve, reject) => {
    let settled = false;
    /** @type {import("node:http").ClientRequest | null} */
    let req = null;
    const timer = setTimeout(() => {
      if (settled) return;
      settled = true;
      req?.destroy(new Error(`timeout after ${timeoutMs}ms`));
      reject(new Error(`timeout after ${timeoutMs}ms fetching ${url}`));
    }, timeoutMs);

    const finish = (err, body) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      if (err) reject(err);
      else resolve(body);
    };

    let parsed;
    try {
      parsed = new URL(url);
    } catch (error) {
      finish(error instanceof Error ? error : new Error(String(error)));
      return;
    }

    const transport = parsed.protocol === "http:" ? httpRequest : httpsRequest;
    req = transport(
      {
        protocol: parsed.protocol,
        hostname: parsed.hostname,
        port: parsed.port || undefined,
        path: `${parsed.pathname}${parsed.search}`,
        method: "GET",
        headers: {
          Accept: "text/plain",
          "User-Agent": "pillow-acp-protocol-drift-check/1",
        },
      },
      (res) => {
        const status = res.statusCode ?? 0;
        if (
          status >= 300
          && status < 400
          && res.headers.location
          && redirectHops > 0
        ) {
          res.resume();
          const next = new URL(res.headers.location, url).toString();
          clearTimeout(timer);
          settled = true;
          httpGetText(next, timeoutMs, redirectHops - 1).then(resolve, reject);
          return;
        }
        /** @type {Buffer[]} */
        const chunks = [];
        res.on("data", (chunk) => {
          chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
        });
        res.on("end", () => {
          const body = Buffer.concat(chunks).toString("utf8");
          if (status < 200 || status >= 300) {
            finish(new Error(`HTTP ${status} for ${url}`));
            return;
          }
          finish(null, body);
        });
        res.on("error", (error) => finish(error));
      },
    );
    req.on("error", (error) => finish(error));
    req.end();
  });
}

/**
 * Load and validate baseline JSON.
 * @param {string} path
 */
export function loadBaseline(path) {
  // Strip UTF-8 BOM if a host tool rewrote the file with one.
  const raw = readFileSync(path, "utf8").replace(/^\uFEFF/u, "");
  const baseline = JSON.parse(raw);
  if (!baseline || baseline.schemaVersion !== 1) {
    throw new Error(`unsupported baseline schema at ${path}`);
  }
  if (!Array.isArray(baseline.crates) || baseline.crates.length === 0) {
    throw new Error(`baseline has no crates: ${path}`);
  }
  for (const c of baseline.crates) {
    if (!c.name || !c.version || !c.checksum) {
      throw new Error(`baseline crate missing name/version/checksum: ${JSON.stringify(c)}`);
    }
  }
  if (!baseline.source?.repository) {
    throw new Error(`baseline missing source.repository: ${path}`);
  }
  return baseline;
}

/**
 * @param {{
 *   baselinePath?: string,
 *   lockfilePath?: string,
 *   lockfileText?: string,
 *   offline?: boolean,
 *   timeoutMs?: number,
 *   fetchImpl?: typeof fetch,
 *   log?: (line: string) => void,
 * }} [options]
 * @returns {Promise<{ status: "ok" | "drift" | "skipped", differences?: string[], reason?: string, url?: string }>}
 */
export async function runAcpDriftCheck(options = {}) {
  const log = options.log ?? console.log;
  const baselinePath = options.baselinePath ?? defaultBaselinePath;
  const baseline = loadBaseline(baselinePath);

  let lockText = options.lockfileText;
  let url;

  if (!lockText && options.lockfilePath) {
    lockText = readFileSync(options.lockfilePath, "utf8");
    log(`ACP_DRIFT_CHECK: using local lockfile ${options.lockfilePath}`);
  }

  if (!lockText) {
    if (options.offline) {
      const reason = "offline flag set";
      log(`${SKIP_MARKER}: ${reason}`);
      return { status: "skipped", reason };
    }
    url = lockfileUrlFromSource(baseline.source);
    log(`ACP_DRIFT_CHECK: fetching ${url}`);
    try {
      lockText = await fetchText(url, {
        timeoutMs: options.timeoutMs,
        fetchImpl: options.fetchImpl,
      });
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error);
      log(`${SKIP_MARKER}: network unavailable or fetch failed (${reason})`);
      return { status: "skipped", reason, url };
    }
  }

  const packages = parseCargoLockPackages(lockText);
  const { ok, differences } = compareBaselineToPackages(baseline.crates, packages);
  if (ok) {
    const names = baseline.crates.map((c) => `${c.name}@${c.version}`).join(", ");
    log(`${OK_MARKER}: ${names}`);
    return { status: "ok", url };
  }

  log(`${DRIFT_MARKER}: upstream Cargo.lock diverges from baseline`);
  for (const line of differences) {
    log(`  - ${line}`);
  }
  log(`  baseline: ${baselinePath}`);
  if (url) log(`  fetched: ${url}`);
  log(
    "  action: re-audit adapter-grok-acp against the new ACP crate pins; update baseline only after review.",
  );
  return { status: "drift", differences, url };
}

async function selfTest() {
  const sampleLock = `
[[package]]
name = "agent-client-protocol"
version = "0.10.4"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "10eeef5e80864f9c3c148a3f395c3e35a66d37ec7561c7845b2bffae8e841759"

[[package]]
name = "agent-client-protocol-schema"
version = "0.11.4"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "ca68e7e55681ce56546c0cecc6bc8f20493d24b44c6d93ec46174f310730bba2"

[[package]]
name = "unrelated"
version = "1.0.0"
`;

  const packages = parseCargoLockPackages(sampleLock);
  if (packages.size !== 3) {
    throw new Error(`expected 3 packages, got ${packages.size}`);
  }

  const baseline = loadBaseline(defaultBaselinePath);
  const match = compareBaselineToPackages(baseline.crates, packages);
  if (!match.ok) {
    throw new Error(`self-test match failed: ${match.differences.join("; ")}`);
  }

  // Mutated version must be reported as drift.
  const drifted = parseCargoLockPackages(
    sampleLock.replace("version = \"0.10.4\"", "version = \"0.99.0\""),
  );
  const drift = compareBaselineToPackages(baseline.crates, drifted);
  if (drift.ok || !drift.differences.some((d) => d.includes("0.10.4 -> 0.99.0"))) {
    throw new Error(`self-test drift detection failed: ${JSON.stringify(drift)}`);
  }

  // Mutated checksum must be reported as drift.
  const badChecksumLock = sampleLock.replace(
    "10eeef5e80864f9c3c148a3f395c3e35a66d37ec7561c7845b2bffae8e841759",
    "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff",
  );
  const badChecksum = compareBaselineToPackages(
    baseline.crates,
    parseCargoLockPackages(badChecksumLock),
  );
  if (badChecksum.ok) {
    throw new Error("self-test checksum drift not detected");
  }

  // Offline path must skip, not fail.
  const lines = [];
  const skipped = await runAcpDriftCheck({
    offline: true,
    log: (line) => lines.push(line),
  });
  if (skipped.status !== "skipped" || !lines.some((l) => l.includes("未核对"))) {
    throw new Error(`self-test offline skip failed: ${JSON.stringify(skipped)} / ${lines}`);
  }

  // Fixture path with matching lock.
  const dir = mkdtempSync(join(tmpdir(), "acp-drift-"));
  try {
    const lockPath = join(dir, "Cargo.lock");
    writeFileSync(lockPath, sampleLock, "utf8");
    const okResult = await runAcpDriftCheck({
      lockfilePath: lockPath,
      log: () => {},
    });
    if (okResult.status !== "ok") {
      throw new Error(`self-test fixture ok failed: ${JSON.stringify(okResult)}`);
    }

    writeFileSync(lockPath, badChecksumLock, "utf8");
    const driftResult = await runAcpDriftCheck({
      lockfilePath: lockPath,
      log: () => {},
    });
    if (driftResult.status !== "drift") {
      throw new Error(`self-test fixture drift failed: ${JSON.stringify(driftResult)}`);
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }

  // URL builder
  const url = lockfileUrlFromSource(baseline.source);
  if (!url.includes("xai-org/grok-build") || !url.endsWith("/Cargo.lock")) {
    throw new Error(`unexpected lock URL: ${url}`);
  }

  console.log(
    `ACP_DRIFT_CHECK_SELF_TEST ${JSON.stringify({
      parse: "passed",
      match: "passed",
      versionDrift: "passed",
      checksumDrift: "passed",
      offlineSkip: "passed",
      fixtureOk: "passed",
      fixtureDrift: "passed",
      url: "passed",
    })}`,
  );
}

function printUsage() {
  console.log(`Usage:
  node scripts/check-acp-protocol-drift.mjs
  node scripts/check-acp-protocol-drift.mjs --offline
  node scripts/check-acp-protocol-drift.mjs --lockfile <path>
  node scripts/check-acp-protocol-drift.mjs --baseline <path>
  node scripts/check-acp-protocol-drift.mjs --self-test

Exit codes:
  0  locks match, or check skipped (offline/network) with 未核对 marker
  1  drift detected, or baseline/self-test failure
  2  usage error
`);
}

const args = process.argv.slice(2);
if (args.includes("--help") || args.includes("-h")) {
  printUsage();
  process.exit(0);
}

if (args.includes("--self-test")) {
  await selfTest();
  process.exit(0);
}

/** @type {Parameters<typeof runAcpDriftCheck>[0]} */
const options = {};
for (let i = 0; i < args.length; i += 1) {
  const arg = args[i];
  if (arg === "--offline") {
    options.offline = true;
  } else if (arg === "--lockfile") {
    options.lockfilePath = args[++i];
    if (!options.lockfilePath) {
      console.error("--lockfile requires a path");
      process.exit(2);
    }
  } else if (arg === "--baseline") {
    options.baselinePath = args[++i];
    if (!options.baselinePath) {
      console.error("--baseline requires a path");
      process.exit(2);
    }
  } else if (arg === "--timeout-ms") {
    options.timeoutMs = Number(args[++i]);
    if (!Number.isFinite(options.timeoutMs) || options.timeoutMs <= 0) {
      console.error("--timeout-ms requires a positive number");
      process.exit(2);
    }
  } else {
    console.error(`unknown argument: ${arg}`);
    printUsage();
    process.exit(2);
  }
}

const result = await runAcpDriftCheck(options);
if (result.status === "drift") {
  process.exit(1);
}
process.exit(0);
