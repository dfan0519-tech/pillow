/**
 * Overbroad process-allow classification and suggestion.
 *
 * Matching lives in the host; these tests still assert the real mis-allow
 * (a saved pattern would auto-allow a different argv), not just string shape.
 */
import { describe, expect, it } from "vitest";
import {
  contrastProcessCommandForOverbroadAllow,
  isOverbroadProcessAllowPattern,
  overbroadProcessAllowWarning,
  processAllowPatternWouldAutoAllow,
  suggestPatternFromSubject,
  suggestProcessAllowPattern,
} from "../src/features/permissions/permissionPattern";
import { encodeProcessSubject } from "../src/features/permissions/processSubject";

describe("isOverbroadProcessAllowPattern", () => {
  it.each([
    "",
    "*",
    "node",
    "node*",
    "node *",
    "node **",
    "no*de",
    "*node",
    "/usr/bin/node *",
    "C:\\nodejs\\node.exe*",
  ])("classifies %j as overbroad", (pattern) => {
    expect(isOverbroadProcessAllowPattern(pattern)).toBe(true);
  });

  it.each([
    "node build.js",
    "node build.js *",
    "cargo test *",
    "cargo test -p keeper-policy",
  ])("classifies %j as not overbroad", (pattern) => {
    expect(isOverbroadProcessAllowPattern(pattern)).toBe(false);
  });

  it("does not treat an exact v1 subject as overbroad", () => {
    expect(
      isOverbroadProcessAllowPattern(encodeProcessSubject("node", ["build.js"])),
    ).toBe(false);
  });
});

describe("suggestProcessAllowPattern — node build.js must not authorize another script", () => {
  const approved = encodeProcessSubject("node", ["build.js"]);
  const other = encodeProcessSubject("node", ["anything-else.js"]);

  it("does not suggest a pattern that would auto-allow node anything-else.js", () => {
    const suggested = suggestProcessAllowPattern(approved);
    expect(isOverbroadProcessAllowPattern(suggested)).toBe(false);
    expect(suggested).not.toBe("node*");
    expect(suggested).not.toBe("node *");
    expect(suggested).not.toBe("node");
    expect(processAllowPatternWouldAutoAllow(suggested, approved)).toBe(true);
    expect(processAllowPatternWouldAutoAllow(suggested, other)).toBe(false);
    expect(
      suggestPatternFromSubject("process", approved, "allow"),
    ).toBe(suggested);
  });

  it("documents that stored node* / node * still auto-allow the other script", () => {
    expect(processAllowPatternWouldAutoAllow("node*", other)).toBe(true);
    expect(processAllowPatternWouldAutoAllow("node *", other)).toBe(true);
    expect(processAllowPatternWouldAutoAllow("node", other)).toBe(true);
  });
});

describe("overbroad contrast example", () => {
  it("derives a real also-allowed command from the approved subject", () => {
    const approved = encodeProcessSubject("node", ["build.js"]);
    const example = contrastProcessCommandForOverbroadAllow("node *", approved);
    expect(example).toBe("node build.js.other");
    expect(
      processAllowPatternWouldAutoAllow(
        "node *",
        encodeProcessSubject("node", ["build.js.other"]),
      ),
    ).toBe(true);
    const warning = overbroadProcessAllowWarning("node*", approved);
    expect(warning).toContain("node build.js.other");
    expect(warning).toMatch(/还会自动放行/);
  });

  it("does not invent a contrast for a narrow suggestion", () => {
    const approved = encodeProcessSubject("node", ["build.js"]);
    const suggested = suggestProcessAllowPattern(approved);
    expect(contrastProcessCommandForOverbroadAllow(suggested, approved)).toBeNull();
    expect(overbroadProcessAllowWarning(suggested, approved)).toBeNull();
  });

  it("deny suggestion is unchanged and may stay broad", () => {
    expect(suggestPatternFromSubject("process", "node", "deny")).toBe("node *");
    expect(
      suggestPatternFromSubject(
        "process",
        encodeProcessSubject("node", ["build.js"]),
        "deny",
      ),
    ).toBe("node build.js *");
  });
});
