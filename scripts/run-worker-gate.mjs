/**
 * Sole compliant Worker gate entry for Pillow.
 *
 * Workers must run this (or `pnpm check` / `pnpm gate`, which invoke it)
 * instead of ad-hoc single-crate cargo commands. See:
 *   docs/verification/worker-gate-entry.md
 *
 * Steps:
 *   1. workspace structure
 *   2. secret / machine-path scan
 *   3. TypeScript typecheck + package tests
 *   4. Rust workspace tests (feature-unification + runtime-host self-dep
 *      ensure test-support seams are compiled)
 *   5. Clippy -D warnings
 *   6. ACP protocol lock drift check (soft-skip offline with 未核对)
 *
 * Does not run desktop E2E (controller-owned). Does not git add/commit.
 */

import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = join(__dirname, "..");
const node = process.execPath;
const isWin = process.platform === "win32";

/**
 * @param {string} command
 * @param {string[]} args
 * @returns {Promise<number>}
 */
function run(command, args, { useShell = false } = {}) {
  return new Promise((resolve, reject) => {
    console.log(`\n==> ${command} ${args.join(" ")}`);
    const child = spawn(command, args, {
      cwd: repositoryRoot,
      env: process.env,
      stdio: "inherit",
      // Shell is only for `.cmd` shims (pnpm/corepack), which cannot be spawned
      // directly on Windows. It must NOT be used for absolute executables:
      // `shell: true` concatenates without quoting, so `process.execPath`
      // ("C:\Program Files\nodejs\node.exe") is split at the space and the step
      // fails with `'C:\Program' is not recognized` — reporting a passing step
      // as FAILED. Node warns about this as DEP0190.
      shell: useShell && isWin,
      windowsHide: true,
    });
    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (signal) {
        resolve(1);
        return;
      }
      resolve(code ?? 1);
    });
  });
}

/**
 * Run a pnpm workspace script. Prefer corepack-managed pnpm when present.
 * @param {string[]} args
 */
async function runPnpm(args) {
  try {
    const code = await run(
      isWin ? "corepack.cmd" : "corepack",
      ["pnpm", ...args],
      { useShell: true },
    );
    // corepack present (including real non-zero test failures).
    return code;
  } catch {
    // corepack missing from PATH — fall back to pnpm.
  }
  return run(isWin ? "pnpm.cmd" : "pnpm", args, { useShell: true });
}

async function main() {
  console.log("PILLOW_WORKER_GATE: start (sole compliant gate entry)");

  /** @type {{ name: string, run: () => Promise<number> }[]} */
  const steps = [
    {
      name: "workspace-structure",
      run: () => run(node, [join("scripts", "check-workspace.mjs")]),
    },
    {
      name: "secrets",
      run: () => run(node, [join("scripts", "check-secrets.mjs")]),
    },
    {
      // Cheap and compile-free, so it runs before anything that builds. The gate
      // previously had clippy but no rustfmt, which let unformatted Rust through
      // a green worker gate and only surfaced in the controller's own pass.
      name: "cargo-fmt",
      run: () =>
        run(node, [
          join("scripts", "run-cargo.mjs"),
          "fmt",
          "--all",
          "--",
          "--check",
        ]),
    },
    {
      name: "typecheck",
      run: () => runPnpm(["typecheck"]),
    },
    {
      name: "package-tests",
      run: () => runPnpm(["test"]),
    },
    {
      name: "cargo-test-workspace",
      run: () =>
        run(node, [join("scripts", "run-cargo.mjs"), "test", "--workspace"]),
    },
    {
      name: "cargo-clippy",
      run: () =>
        run(node, [
          join("scripts", "run-cargo.mjs"),
          "clippy",
          "--workspace",
          "--",
          "-D",
          "warnings",
        ]),
    },
    {
      name: "acp-protocol-drift",
      run: () => run(node, [join("scripts", "check-acp-protocol-drift.mjs")]),
    },
  ];

  for (const step of steps) {
    const code = await step.run();
    if (code !== 0) {
      console.error(
        `PILLOW_WORKER_GATE: FAILED at step "${step.name}" (exit ${code})`,
      );
      process.exit(code);
    }
  }

  console.log("PILLOW_WORKER_GATE: OK");
  process.exit(0);
}

const args = process.argv.slice(2);
if (args.includes("--help") || args.includes("-h")) {
  console.log(`Usage: node scripts/run-worker-gate.mjs

Sole compliant Worker gate. Equivalent: pnpm check | pnpm gate

See docs/verification/worker-gate-entry.md`);
  process.exit(0);
}

await main();
