/**
 * Renderer half of the cross-language process-subject conformance check.
 *
 * The renderer has to suggest and display permission-rule patterns without
 * calling the host, so it carries its own copy of the v1 subject encoding.
 * Two implementations of one wire format is exactly how a seam rots: each
 * side's own tests stay green while the pair drifts apart. That already
 * happened once on this project, on the OAuth login chain — transport and
 * creation path were both green and the join between them was untested.
 *
 * Both sides read the same vector file, so drift on either side turns a test
 * red here or in `crates/kernel-policy/tests/process_subject_conformance.rs`.
 */
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import {
  decodeProcessSubject,
  encodeProcessSubject,
  formatProcessSubjectForDisplay,
  processSubjectIsEncoded,
  spaceJoinIsBijective,
} from "../src/features/permissions/processSubject";
import { suggestPatternFromSubject } from "../src/features/permissions/permissionPattern";

type Vector = {
  name: string;
  program: string;
  argv: string[];
  subject: string;
};

type NegativeCase = { name: string; subject: string };

const fixturePath = resolve(
  dirname(fileURLToPath(import.meta.url)),
  "../../../crates/kernel-policy/tests/fixtures/process-subject-v1.json",
);

const fixture = JSON.parse(readFileSync(fixturePath, "utf8")) as {
  vectors: Vector[];
  undecodable: NegativeCase[];
};

describe("process subject v1 — host conformance", () => {
  it("keeps enough vectors to be worth trusting", () => {
    // A shrinking fixture is how this check quietly stops checking anything.
    expect(fixture.vectors.length).toBeGreaterThanOrEqual(10);
    expect(fixture.undecodable.length).toBeGreaterThanOrEqual(10);
  });

  it.each(fixture.vectors)("encodes $name", ({ program, argv, subject }) => {
    expect(encodeProcessSubject(program, argv)).toBe(subject);
  });

  it.each(fixture.vectors)("decodes $name", ({ program, argv, subject }) => {
    expect(decodeProcessSubject(subject)).toEqual([program, ...argv]);
  });

  it.each(fixture.undecodable)("refuses to decode $name", ({ subject }) => {
    expect(decodeProcessSubject(subject)).toBeNull();
  });
});

describe("process subject v1 — renderer consumers", () => {
  it("suggests a readable pattern when argv survives a space join", () => {
    const subject = encodeProcessSubject("cargo", ["test", "--all"]);
    expect(suggestPatternFromSubject("process", subject)).toBe("cargo test *");
  });

  it("suggests a bare program with a wildcard when argv is empty", () => {
    expect(suggestPatternFromSubject("process", "cargo")).toBe("cargo *");
  });

  it("never splits an encoded subject on whitespace", () => {
    // The pre-EA-02 renderer did `subject.split(/\s+/)`, which slices through
    // the version mark and yields a rule that can never match anything.
    const subject = encodeProcessSubject("node", ["safe.js", "--version"]);
    const pattern = suggestPatternFromSubject("process", subject);
    expect(pattern).toBe("node safe.js *");
    expect(processSubjectIsEncoded(pattern)).toBe(false);
  });

  it("falls back to the exact subject when argv cannot be space-joined", () => {
    // `node safe.js *` would also match ["safe.js", "--version"] — a different
    // command. Only the exact encoding means this one argv.
    const subject = encodeProcessSubject("node", ["safe.js --version"]);
    expect(spaceJoinIsBijective(["safe.js --version"])).toBe(false);
    expect(suggestPatternFromSubject("process", subject)).toBe(subject);
  });

  it("keeps a legacy space-joined subject verbatim instead of guessing", () => {
    // It will not match host-side. Inventing a shape for it would tell the
    // user they saved a working rule when they did not.
    expect(suggestPatternFromSubject("process", "node --version")).toBe(
      "node --version",
    );
  });

  it("does not trim a subject whose last argument ends in whitespace", () => {
    const subject = encodeProcessSubject("printf", ["a "]);
    expect(suggestPatternFromSubject("process", subject)).toBe(subject);
    expect(decodeProcessSubject(subject)).toEqual(["printf", "a "]);
  });

  it("renders an encoded subject without control characters", () => {
    const subject = encodeProcessSubject("node", ["safe.js --version"]);
    expect(processSubjectIsEncoded(subject)).toBe(true);
    const display = formatProcessSubjectForDisplay(subject);
    expect(display).toBe('node "safe.js --version"');
    expect(processSubjectIsEncoded(display)).toBe(false);
  });

  it("shows an empty argument rather than dropping it", () => {
    const subject = encodeProcessSubject("printf", ["", "x"]);
    expect(formatProcessSubjectForDisplay(subject)).toBe('printf "" x');
  });

  it("shows an undecodable subject verbatim rather than inventing one", () => {
    expect(formatProcessSubjectForDisplay("node --version")).toBe(
      "node --version",
    );
  });
});
