/**
 * C4 D3B4 — Tauri Git gateway invoke contract and hostile DTO rejection.
 * Proves exact command names/payloads and strict path-free public errors.
 */
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  GIT_ERROR_MESSAGES,
  GIT_PUBLIC_ERRORS,
  GitGatewayError,
  isGitPublicError,
  normalizeGitError,
  type GitDiffView,
  type GitDiscoveryView,
  type GitGeneration,
  type GitStatusView,
} from "../src/lib/gitGateway";
import {
  normalizeGitDiff,
  normalizeGitDiscovery,
  normalizeGitStatus,
  TauriGitGateway,
} from "../src/lib/tauriGitGateway";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const projectId = "00000000-0000-0000-0000-0000000000p1";
const expected: GitGeneration = { repositoryId: ".", generation: 7 };

const discovery: GitDiscoveryView = {
  generation: 7,
  repositories: [
    { repositoryId: ".", available: true },
    { repositoryId: "packages/nested", available: true },
    { repositoryId: "vendor/frozen", available: false },
  ],
};

const status: GitStatusView = {
  repositoryId: ".",
  generation: 7,
  branch: "main",
  detached: false,
  headShort: "abc1234",
  dirtyCount: 2,
  files: [
    { path: "src/lib/gitGateway.ts", kind: "modified" },
    {
      path: "src/lib/tauriGitGateway.ts",
      kind: "renamed",
      originalPath: "src/lib/oldGitGateway.ts",
    },
  ],
  truncated: false,
};

function textDiff(overrides: Partial<GitDiffView> = {}): GitDiffView {
  const unified = "diff --git a/a.ts b/a.ts\n@@ -1 +1 @@\n-old\n+new\n";
  const base: GitDiffView = {
    repositoryId: ".",
    generation: 7,
    path: "a.ts",
    kind: "text",
    unified,
    truncated: false,
    lineCount: 4,
    byteCount: new TextEncoder().encode(unified).length,
  };
  return { ...base, ...overrides };
}

function binaryDiff(overrides: Partial<GitDiffView> = {}): GitDiffView {
  return {
    repositoryId: ".",
    generation: 7,
    path: "assets/icon.png",
    kind: "binary",
    truncated: false,
    ...overrides,
  };
}

describe("TauriGitGateway invoke contract", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("invokes discover/status/diff with exact names and camelCase keys only", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce(discovery)
      .mockResolvedValueOnce(status)
      .mockResolvedValueOnce(textDiff());

    const gateway = new TauriGitGateway();
    await gateway.discoverGitRepositories(projectId);
    await gateway.getGitStatus({ projectId, expected });
    await gateway.openGitDiff({ projectId, expected, path: "a.ts" });

    expect(invoke).toHaveBeenNthCalledWith(1, "discover_git_repositories", {
      projectId,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "get_git_status", {
      projectId,
      repositoryId: ".",
      generation: 7,
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "open_git_diff", {
      projectId,
      repositoryId: ".",
      generation: 7,
      path: "a.ts",
    });

    for (const call of vi.mocked(invoke).mock.calls) {
      const args = call[1] as Record<string, unknown> | undefined;
      if (!args) continue;
      expect(args).not.toHaveProperty("root");
      expect(args).not.toHaveProperty("rootPath");
      expect(args).not.toHaveProperty("executable");
      expect(args).not.toHaveProperty("gitPath");
      expect(args).not.toHaveProperty("argv");
      expect(args).not.toHaveProperty("cwd");
      expect(args).not.toHaveProperty("nativePath");
      expect(Object.keys(args).every((key) =>
        ["projectId", "repositoryId", "generation", "path"].includes(key),
      )).toBe(true);
    }
  });

  it("accepts valid discovery, status, and text/binary/unsupported diffs", async () => {
    const unsupported: GitDiffView = {
      repositoryId: ".",
      generation: 7,
      path: "huge.bin",
      kind: "unsupported",
      truncated: true,
    };
    vi.mocked(invoke)
      .mockResolvedValueOnce(discovery)
      .mockResolvedValueOnce(status)
      .mockResolvedValueOnce(textDiff())
      .mockResolvedValueOnce(binaryDiff())
      .mockResolvedValueOnce(unsupported);

    const gateway = new TauriGitGateway();
    await expect(gateway.discoverGitRepositories(projectId)).resolves.toEqual(discovery);
    await expect(gateway.getGitStatus({ projectId, expected })).resolves.toEqual(status);
    await expect(
      gateway.openGitDiff({ projectId, expected, path: "a.ts" }),
    ).resolves.toMatchObject({ kind: "text", unified: expect.any(String) });
    await expect(
      gateway.openGitDiff({ projectId, expected, path: "assets/icon.png" }),
    ).resolves.toEqual(binaryDiff());
    await expect(
      gateway.openGitDiff({ projectId, expected, path: "huge.bin" }),
    ).resolves.toEqual(unsupported);
  });
});

describe("normalizeGitDiscovery hostile responses", () => {
  it("rejects unknown fields and non-portable repository ids", () => {
    expect(() =>
      normalizeGitDiscovery({
        ...discovery,
        nativeRoot: "F:\\secret",
      }),
    ).toThrow(GitGatewayError);

    expect(() =>
      normalizeGitDiscovery({
        generation: 1,
        repositories: [{ repositoryId: "C:/native", available: true }],
      }),
    ).toThrow(GitGatewayError);

    expect(() =>
      normalizeGitDiscovery({
        generation: 1,
        repositories: [{ repositoryId: "\\\\server\\share", available: true }],
      }),
    ).toThrow(GitGatewayError);

    expect(() =>
      normalizeGitDiscovery({
        generation: 1,
        repositories: [{ repositoryId: "foo\\bar", available: true }],
      }),
    ).toThrow(GitGatewayError);

    expect(() =>
      normalizeGitDiscovery({
        generation: 1,
        repositories: [{ repositoryId: "../escape", available: true }],
      }),
    ).toThrow(GitGatewayError);

    expect(() =>
      normalizeGitDiscovery({
        generation: 1,
        repositories: [{ repositoryId: "/abs", available: true }],
      }),
    ).toThrow(GitGatewayError);

    expect(() =>
      normalizeGitDiscovery({
        generation: 1,
        repositories: [{ repositoryId: ".", available: true, root: "F:\\leak" }],
      }),
    ).toThrow(GitGatewayError);
  });

  it("rejects unsafe integers and oversized repository lists", () => {
    expect(() =>
      normalizeGitDiscovery({ generation: 1.5, repositories: [] }),
    ).toThrow(GitGatewayError);
    expect(() =>
      normalizeGitDiscovery({ generation: -1, repositories: [] }),
    ).toThrow(GitGatewayError);
    expect(() =>
      normalizeGitDiscovery({ generation: Number.MAX_SAFE_INTEGER + 1, repositories: [] }),
    ).toThrow(GitGatewayError);
    expect(() =>
      normalizeGitDiscovery({
        generation: 1,
        repositories: Array.from({ length: 33 }, (_, i) => ({
          repositoryId: `pkg-${i}`,
          available: true,
        })),
      }),
    ).toThrow(GitGatewayError);
  });

  it("accepts repository id '.' and portable nested ids", () => {
    expect(
      normalizeGitDiscovery({
        generation: 0,
        repositories: [
          { repositoryId: ".", available: true },
          { repositoryId: "apps/desktop", available: false },
        ],
      }),
    ).toEqual({
      generation: 0,
      repositories: [
        { repositoryId: ".", available: true },
        { repositoryId: "apps/desktop", available: false },
      ],
    });
  });
});

describe("normalizeGitStatus hostile responses", () => {
  it("rejects extra fields, invalid enums, and non-portable paths", () => {
    expect(() =>
      normalizeGitStatus({ ...status, debug: true }, expected),
    ).toThrow(GitGatewayError);

    expect(() =>
      normalizeGitStatus(
        {
          ...status,
          files: [{ path: "a.ts", kind: "mutated" }],
        },
        expected,
      ),
    ).toThrow(GitGatewayError);

    expect(() =>
      normalizeGitStatus(
        {
          ...status,
          files: [{ path: "C:/native/a.ts", kind: "modified" }],
        },
        expected,
      ),
    ).toThrow(GitGatewayError);

    expect(() =>
      normalizeGitStatus(
        {
          ...status,
          files: [{ path: "a.ts", kind: "modified", originalPath: "..\\secret" }],
        },
        expected,
      ),
    ).toThrow(GitGatewayError);

    expect(() =>
      normalizeGitStatus(
        {
          ...status,
          files: [{ path: "a.ts", kind: "modified", executable: true }],
        },
        expected,
      ),
    ).toThrow(GitGatewayError);
  });

  it("rejects oversized file lists and unsafe dirtyCount", () => {
    expect(() =>
      normalizeGitStatus(
        {
          ...status,
          dirtyCount: 5_001,
          files: [],
        },
        expected,
      ),
    ).toThrow(GitGatewayError);

    expect(() =>
      normalizeGitStatus(
        {
          ...status,
          dirtyCount: 1,
          files: Array.from({ length: 5_001 }, (_, i) => ({
            path: `f-${i}.ts`,
            kind: "modified" as const,
          })),
        },
        expected,
      ),
    ).toThrow(GitGatewayError);
  });

  it("keeps git_generation_stale and git_generation_mismatch from strict validation", () => {
    try {
      normalizeGitStatus({ ...status, generation: 99 }, expected);
      expect.fail("expected throw");
    } catch (error) {
      expect(error).toBeInstanceOf(GitGatewayError);
      expect((error as GitGatewayError).code).toBe("git_generation_stale");
      expect((error as GitGatewayError).message).toBe(
        GIT_ERROR_MESSAGES.git_generation_stale,
      );
      expect((error as GitGatewayError).message).not.toMatch(/[\\/]/);
    }

    try {
      normalizeGitStatus(
        { ...status, repositoryId: "packages/other" },
        expected,
      );
      expect.fail("expected throw");
    } catch (error) {
      expect(error).toBeInstanceOf(GitGatewayError);
      expect((error as GitGatewayError).code).toBe("git_generation_mismatch");
      expect((error as GitGatewayError).message).toBe(
        GIT_ERROR_MESSAGES.git_generation_mismatch,
      );
    }
  });
});

describe("normalizeGitDiff hostile responses", () => {
  it("rejects line/byte inconsistency and oversized text", () => {
    expect(() =>
      normalizeGitDiff(textDiff({ byteCount: 1 }), expected),
    ).toThrow(GitGatewayError);

    expect(() =>
      normalizeGitDiff(textDiff({ lineCount: 99 }), expected),
    ).toThrow(GitGatewayError);

    const huge = "x".repeat(256 * 1024 + 1);
    expect(() =>
      normalizeGitDiff(
        textDiff({
          unified: huge,
          lineCount: 1,
          byteCount: huge.length,
        }),
        expected,
      ),
    ).toThrow(GitGatewayError);
  });

  it("rejects binary/unsupported responses that inject unified text or counts", () => {
    expect(() =>
      normalizeGitDiff(
        binaryDiff({
          unified: "diff --git invented\n",
          lineCount: 1,
          byteCount: 20,
        } as GitDiffView),
        expected,
      ),
    ).toThrow(GitGatewayError);

    expect(() =>
      normalizeGitDiff(
        {
          repositoryId: ".",
          generation: 7,
          path: "a.bin",
          kind: "unsupported",
          truncated: false,
          unified: "",
        },
        expected,
      ),
    ).toThrow(GitGatewayError);

    expect(() =>
      normalizeGitDiff(
        {
          repositoryId: ".",
          generation: 7,
          path: "a.bin",
          kind: "binary",
          truncated: false,
          lineCount: 0,
        },
        expected,
      ),
    ).toThrow(GitGatewayError);

    expect(() =>
      normalizeGitDiff(
        {
          repositoryId: ".",
          generation: 7,
          path: "a.bin",
          kind: "binary",
          truncated: false,
          byteCount: 0,
        },
        expected,
      ),
    ).toThrow(GitGatewayError);
  });

  it("requires text diffs to carry unified + consistent counts", () => {
    const { unified: _u, ...missingUnified } = textDiff();
    expect(() => normalizeGitDiff(missingUnified, expected)).toThrow(GitGatewayError);

    const { lineCount: _l, ...missingLines } = textDiff();
    expect(() => normalizeGitDiff(missingLines, expected)).toThrow(GitGatewayError);

    const { byteCount: _b, ...missingBytes } = textDiff();
    expect(() => normalizeGitDiff(missingBytes, expected)).toThrow(GitGatewayError);
  });

  it("rejects native/backslash/traversal paths on diffs", () => {
    for (const path of [
      "C:/Windows/system32",
      "<repo>\\secret",
      "../escape",
      "foo\\bar",
      "/etc/passwd",
      "a/../../b",
    ]) {
      expect(() =>
        normalizeGitDiff(textDiff({ path }), expected),
      ).toThrow(GitGatewayError);
    }
  });

  it("preserves generation mismatch/stale codes for diffs", () => {
    expect(() =>
      normalizeGitDiff(textDiff({ generation: 1 }), expected),
    ).toThrow(
      expect.objectContaining({
        name: "GitGatewayError",
        code: "git_generation_stale",
      }),
    );
    expect(() =>
      normalizeGitDiff(textDiff({ repositoryId: "other" }), expected),
    ).toThrow(
      expect.objectContaining({
        name: "GitGatewayError",
        code: "git_generation_mismatch",
      }),
    );
  });
});

describe("public error normalization", () => {
  it("exposes exactly the six accepted public error codes", () => {
    expect([...GIT_PUBLIC_ERRORS]).toEqual([
      "git_project_not_found",
      "git_path_rejected",
      "git_repository_not_found",
      "git_unavailable",
      "git_generation_stale",
      "git_generation_mismatch",
    ]);
    for (const code of GIT_PUBLIC_ERRORS) {
      expect(isGitPublicError(code)).toBe(true);
      expect(GIT_ERROR_MESSAGES[code]).toBeTruthy();
      expect(GIT_ERROR_MESSAGES[code]).not.toMatch(/[\\/]/);
      expect(GIT_ERROR_MESSAGES[code]).not.toMatch(/F:/i);
    }
    expect(isGitPublicError("git_secret_leak")).toBe(false);
  });

  it("maps known host codes to fixed path-free messages and never echoes host text", async () => {
    const secret =
      "failed at F:\\secret\\project\\src\\a.ts grant=super-secret-token";
    const gateway = new TauriGitGateway();
    for (const code of GIT_PUBLIC_ERRORS) {
      vi.mocked(invoke).mockRejectedValueOnce({ code, message: secret });
      try {
        await gateway.discoverGitRepositories(projectId);
        expect.fail("expected rejection");
      } catch (error) {
        expect(error).toBeInstanceOf(GitGatewayError);
        expect((error as GitGatewayError).code).toBe(code);
        expect((error as GitGatewayError).message).toBe(GIT_ERROR_MESSAGES[code]);
        expect((error as GitGatewayError).message).not.toContain("F:");
        expect((error as GitGatewayError).message).not.toContain("\\");
        expect((error as GitGatewayError).message).not.toContain("super-secret");
        expect((error as GitGatewayError).message).not.toContain("src/a.ts");
      }
    }
  });

  it("maps unknown/malformed Tauri rejections to path-free git_unavailable", async () => {
    const gateway = new TauriGitGateway();
    for (const payload of [
      new Error("native path F:\\secret leaked"),
      { code: "sqlite_error", message: "F:\\db.sqlite" },
      { code: "git_secret", message: "leak" },
      "unexpected",
      null,
      42,
    ]) {
      vi.mocked(invoke).mockRejectedValueOnce(payload);
      await expect(gateway.getGitStatus({ projectId, expected })).rejects.toMatchObject({
        name: "GitGatewayError",
        code: "git_unavailable",
        message: GIT_ERROR_MESSAGES.git_unavailable,
      });
    }
  });

  it("normalizes invalid responses to git_unavailable while preserving generation codes through the gateway", async () => {
    const gateway = new TauriGitGateway();

    vi.mocked(invoke).mockResolvedValueOnce({
      ...status,
      files: [{ path: "C:/native", kind: "modified" }],
    });
    await expect(gateway.getGitStatus({ projectId, expected })).rejects.toMatchObject({
      code: "git_unavailable",
      message: GIT_ERROR_MESSAGES.git_unavailable,
    });

    vi.mocked(invoke).mockResolvedValueOnce({ ...status, generation: 1 });
    await expect(gateway.getGitStatus({ projectId, expected })).rejects.toMatchObject({
      code: "git_generation_stale",
      message: GIT_ERROR_MESSAGES.git_generation_stale,
    });

    vi.mocked(invoke).mockResolvedValueOnce({
      ...status,
      repositoryId: "packages/other",
    });
    await expect(gateway.getGitStatus({ projectId, expected })).rejects.toMatchObject({
      code: "git_generation_mismatch",
      message: GIT_ERROR_MESSAGES.git_generation_mismatch,
    });

    vi.mocked(invoke).mockResolvedValueOnce(
      binaryDiff({
        unified: "invented",
        lineCount: 1,
        byteCount: 8,
      } as GitDiffView),
    );
    await expect(
      gateway.openGitDiff({ projectId, expected, path: "assets/icon.png" }),
    ).rejects.toMatchObject({
      code: "git_unavailable",
    });
  });

  it("normalizeGitError rewrites instance messages to fixed path-free copy", () => {
    const stale = new GitGatewayError(
      "git_generation_stale",
      "stale at F:\\secret\\repo",
    );
    const normalized = normalizeGitError(stale);
    expect(normalized.code).toBe("git_generation_stale");
    expect(normalized.message).toBe(GIT_ERROR_MESSAGES.git_generation_stale);
    expect(normalized.message).not.toContain("F:");
    expect(normalized).not.toBe(stale);
  });
});

describe("untracked directory entries survive DTO validation", () => {
  // Regression: the same portable-path rule is implemented in the parser, the
  // host project-path mapping and here. Only fixing the first two still left a
  // collapsed untracked directory looking malformed to the renderer, which
  // degrades the whole status to `git_unavailable`.
  it("accepts a collapsed untracked directory path in status entries", () => {
    const view = normalizeGitStatus(
      {
        ...status,
        dirtyCount: 2,
        files: [
          { path: "vendor/", kind: "untracked" },
          { path: "notes/todo.md", kind: "untracked" },
        ],
      },
      expected,
    );
    expect(view.files.map((f) => f.path)).toEqual(["vendor/", "notes/todo.md"]);
  });

  it("still rejects malformed separators and traversal in status entries", () => {
    for (const bad of ["/", "//", "vendor//", "./", "../escape/", "a\b", "C:/x/"]) {
      expect(() =>
        normalizeGitStatus(
          { ...status, dirtyCount: 1, files: [{ path: bad, kind: "untracked" }] },
          expected,
        ),
      ).toThrow();
    }
  });

  it("does not accept a directory form as a diff target", () => {
    expect(() =>
      normalizeGitDiff(
        {
          repositoryId: ".",
          generation: 7,
          path: "vendor/",
          kind: "text",
          truncated: false,
          text: "",
        },
        expected,
      ),
    ).toThrow();
  });
});
