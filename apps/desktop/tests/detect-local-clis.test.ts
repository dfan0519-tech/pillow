/**
 * Local CLI detect — closed DTO parse + auth three-state labels.
 * unknown must never render as 未登录.
 */
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  LOCAL_CLI_DETECTION_DTO_KEYS,
  formatLocalCliAuthLabel,
  parseLocalCliDetectionView,
  TauriRuntimeSlotsGateway,
} from "../src/features/settings/runtimeSlotsGateway";
import {
  LOCAL_CLI_AUTH_AUTHENTICATED_LABEL,
  LOCAL_CLI_AUTH_NOT_AUTHENTICATED_LABEL,
  LOCAL_CLI_AUTH_UNKNOWN_LABEL,
} from "../src/features/settings/runtimeSlotsTypes";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("parseLocalCliDetectionView — closed keys", () => {
  it("accepts a closed authenticated row and rejects smuggled secret keys", () => {
    const good = {
      kind: "codex",
      found: true,
      executablePath: "C:/tools/codex.exe",
      version: "0.1.0",
      authStatus: "authenticated",
      accountLabel: "ChatGPT",
      authReason: null,
    };
    const view = parseLocalCliDetectionView(good);
    expect(view.kind).toBe("codex");
    expect(view.authStatus).toBe("authenticated");
    expect(view.accountLabel).toBe("ChatGPT");
    expect(Object.keys(good).sort()).toEqual(
      [...LOCAL_CLI_DETECTION_DTO_KEYS].sort(),
    );

    expect(() =>
      parseLocalCliDetectionView({ ...good, apiKey: "smuggle" }),
    ).toThrow(/secret|invalid/i);
  });

  it("rejects unknown status with accountLabel (host invariant)", () => {
    expect(() =>
      parseLocalCliDetectionView({
        kind: "gemini",
        found: true,
        executablePath: "/bin/gemini",
        version: null,
        authStatus: "unknown",
        accountLabel: "should-not",
        authReason: "no_headless_status_command",
      }),
    ).toThrow();
  });
});

describe("formatLocalCliAuthLabel — unknown ≠ 未登录", () => {
  it("positive: not_authenticated shows 未登录", () => {
    const text = formatLocalCliAuthLabel({
      authStatus: "not_authenticated",
      accountLabel: null,
      authReason: null,
    });
    expect(text).toBe(LOCAL_CLI_AUTH_NOT_AUTHENTICATED_LABEL);
    expect(text).toBe("未登录");
  });

  it("positive: authenticated shows 已登录 and optional label", () => {
    expect(
      formatLocalCliAuthLabel({
        authStatus: "authenticated",
        accountLabel: "user@example.com",
        authReason: null,
      }),
    ).toContain(LOCAL_CLI_AUTH_AUTHENTICATED_LABEL);
    expect(
      formatLocalCliAuthLabel({
        authStatus: "authenticated",
        accountLabel: "user@example.com",
        authReason: null,
      }),
    ).toContain("user@example.com");
  });

  it("positive: unknown shows 未确定 + reason", () => {
    const text = formatLocalCliAuthLabel({
      authStatus: "unknown",
      accountLabel: null,
      authReason: "timed_out",
    });
    expect(text).toContain(LOCAL_CLI_AUTH_UNKNOWN_LABEL);
    expect(text).toContain("timed_out");
    // Negative pin: never the logged-out string.
    expect(text).not.toBe(LOCAL_CLI_AUTH_NOT_AUTHENTICATED_LABEL);
    expect(text).not.toContain("未登录");
  });

  it("negative: timeout / not_found / no_headless never map to 未登录", () => {
    for (const reason of [
      "timed_out",
      "not_found",
      "no_headless_status_command",
      "spawn_failed",
    ]) {
      const text = formatLocalCliAuthLabel({
        authStatus: "unknown",
        accountLabel: null,
        authReason: reason,
      });
      expect(text).toContain("未确定");
      expect(text).not.toContain("未登录");
    }
  });
});

describe("TauriRuntimeSlotsGateway.detectLocalClis", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("invokes detect_local_clis and parses closed rows", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      {
        kind: "grok",
        found: false,
        executablePath: null,
        version: null,
        authStatus: "unknown",
        accountLabel: null,
        authReason: "not_found",
      },
    ]);
    const gateway = new TauriRuntimeSlotsGateway();
    const rows = await gateway.detectLocalClis();
    expect(invoke).toHaveBeenCalledWith("detect_local_clis", {});
    expect(rows).toHaveLength(1);
    expect(rows[0]!.authStatus).toBe("unknown");
    expect(formatLocalCliAuthLabel(rows[0]!)).not.toContain("未登录");
  });
});
