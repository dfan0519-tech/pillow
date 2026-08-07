/**
 * Repository secret / machine-path gate.
 *
 * `AGENTS.md` forbids committing credentials, account tokens, machine-specific
 * paths and runtime session data. This gate enforces that mechanically.
 *
 * It self-tests first. A scanner that silently matches nothing is worse than no
 * scanner: during the C4 checkpoint a hand-written regex reported "clean" while
 * a developer-specific Git path sat in product code. Every pattern below is
 * therefore proven against a known-positive sample before the real scan runs,
 * and the gate fails closed if any pattern stops detecting.
 */

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync as read } from "node:fs";

const CODE_EXT = /\.(rs|ts|tsx|mjs|cjs|js|jsx|json|toml|yml|yaml)$/u;

/**
 * `block` patterns fail the gate anywhere in the repository.
 * `codeOnly` patterns fail only inside code files, where a machine-specific
 * path is a portability defect rather than a documentation convention.
 */
const RULES = [
  {
    id: "xai-key",
    scope: "block",
    re: /xai-[A-Za-z0-9_-]{20,}/u,
    sample: "xai-abcdefghijklmnopqrstuvwxyz012345",
    why: "xAI API key",
  },
  {
    id: "openai-key",
    scope: "block",
    re: /\bsk-[A-Za-z0-9_-]{20,}/u,
    sample: "sk-abcdefghijklmnopqrstuvwxyz012345",
    why: "OpenAI-style API key",
  },
  {
    id: "github-token",
    scope: "block",
    re: /\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{20,}/u,
    sample: "ghp_abcdefghijklmnopqrstuvwxyz0123456789",
    why: "GitHub token",
  },
  {
    id: "private-key",
    scope: "block",
    re: /-----BEGIN (?:[A-Z ]+ )?PRIVATE KEY-----/u,
    sample: "-----BEGIN RSA PRIVATE KEY-----",
    why: "private key block",
  },
  {
    id: "bearer",
    scope: "block",
    re: /\bBearer\s+[A-Za-z0-9._-]{20,}/u,
    sample: "Bearer abcdefghijklmnopqrstuvwxyz0123",
    why: "bearer token",
  },
  {
    id: "keeper-session",
    scope: "block",
    re: /\b\d{8}T\d{6}Z-[a-z]+-[a-z0-9]{4,}\b/u,
    sample: "20260807T035151Z-pillow-pxrm",
    why: "Keeper session id (runtime session data)",
  },
  {
    id: "keeper-lease",
    scope: "block",
    re: /\bgrok-lease-[a-z0-9]+-[a-z0-9]+\b/u,
    sample: "grok-lease-msij5g0v-fc6b73",
    why: "Keeper slot lease id (runtime session data)",
  },
  {
    id: "user-profile-path",
    scope: "block",
    re: /[A-Za-z]:[\\/]Users[\\/](?!Alice\b)[A-Za-z0-9._-]+/u,
    sample: String.raw`%USERPROFILE%\.cargo`,
    why: "host user-profile path",
  },
  {
    id: "machine-drive-path",
    scope: "codeOnly",
    // Absolute drive paths are everywhere in this repo's tests as *synthetic*
    // fixtures asserting traversal/redaction rejection (`C:\project\..\escape`,
    // `C:\pillow-missing-git-not-real\git.exe`). Those are correct and must not
    // be flagged. What must be flagged is a path that is real on the
    // committer's own machine — that is precisely the leak. So the regex only
    // proposes a candidate and `verify` decides: the path has to actually exist
    // here and live outside the standard system locations.
    //
    // This check is deliberately machine-relative. It answers "is this YOUR
    // path?", which cannot be decided from the string alone.
    re: /[A-Za-z]:[\\/][A-Za-z0-9._ -]+(?:[\\/][A-Za-z0-9._ -]+)*/u,
    verify: (hit) => {
      const norm = hit.replace(/\//gu, "\\");
      const system = [
        "c:\\windows",
        "c:\\program files",
        "c:\\program files (x86)",
        "c:\\programdata",
        "c:\\users",
      ];
      const lower = norm.toLowerCase();
      if (system.some((s) => lower.startsWith(s))) return false;
      return existsSync(norm);
    },
    sample: process.cwd(),
    why: "absolute path that exists on this machine (developer-specific)",
  },
];

/**
 * Files whose matches are intentional and reviewed. Keep this list short and
 * always state why — an allowlist is where a real leak hides.
 */
const ALLOW = new Map([
  [
    "scripts/check-secrets.mjs",
    "this gate; contains every pattern and its self-test sample by design",
  ],
  [
    "crates/adapter-grok-acp/src/map/terminal.rs",
    "synthetic `C:\\Users\\Alice` fixture asserting redaction; not a real host path",
  ],
]);

function selfTest() {
  const broken = [];
  for (const rule of RULES) {
    const m = rule.sample.match(rule.re);
    const detected = Boolean(m) && (!rule.verify || rule.verify(m[0]));
    if (!detected) {
      broken.push(`${rule.id}: failed to detect its own sample`);
    }
  }
  // A pattern that matches everything is equally useless.
  const benign = "const greeting = 'hello world';";
  for (const rule of RULES) {
    const m = benign.match(rule.re);
    if (m && (!rule.verify || rule.verify(m[0]))) {
      broken.push(`${rule.id}: matches benign text`);
    }
  }
  if (broken.length) {
    console.error("secret gate self-test FAILED:");
    for (const b of broken) console.error(`  - ${b}`);
    process.exit(2);
  }
  return RULES.length;
}

function trackedFiles() {
  return execFileSync("git", ["ls-files", "-z"], {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  })
    .split("\0")
    .filter(Boolean);
}

const patternCount = selfTest();

const findings = [];
for (const file of trackedFiles()) {
  if (ALLOW.has(file)) continue;
  let text;
  try {
    text = read(file, "utf8");
  } catch {
    continue; // binary or unreadable
  }
  if (text.includes("\0")) continue;
  const isCode = CODE_EXT.test(file);
  const lines = text.split(/\r?\n/u);
  for (const rule of RULES) {
    if (rule.scope === "codeOnly" && !isCode) continue;
    for (let i = 0; i < lines.length; i += 1) {
      const m = lines[i].match(rule.re);
      if (m && (!rule.verify || rule.verify(m[0]))) {
        findings.push({ file, line: i + 1, id: rule.id, why: rule.why });
        break; // one finding per rule per file is enough to fail
      }
    }
  }
}

if (findings.length) {
  console.error(`secret gate FAILED — ${findings.length} finding(s):`);
  for (const f of findings) {
    console.error(`  ${f.file}:${f.line}  [${f.id}] ${f.why}`);
  }
  console.error(
    "\nRedact the value, or add a reviewed entry to ALLOW in scripts/check-secrets.mjs.",
  );
  process.exit(1);
}

console.log(
  `secret gate ok — ${patternCount} patterns self-tested, no findings in tracked files`,
);
