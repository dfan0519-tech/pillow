/**
 * Two-tier permission policy gateway: host DTO allowlist + no fabricated fallback.
 */
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  TauriSettingsGateway,
  parsePermissionPolicy,
} from "../src/features/settings/settingsGateway";
import { SettingsGatewayError } from "../src/features/settings/settingsTypes";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const CLOSED_POLICY = {
  bypassAvailable: false,
  bypassAvailableSource: "global",
  projectMode: "bypass",
  projectModeSource: "project",
  effectiveMode: "default",
} as const;

describe("TauriSettingsGateway permission policy", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("invokes get_permission_policy and accepts the closed-key DTO", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({ ...CLOSED_POLICY });
    const gateway = new TauriSettingsGateway();
    const policy = await gateway.getPermissionPolicy("project-1");
    expect(invoke).toHaveBeenCalledWith("get_permission_policy", {
      projectId: "project-1",
    });
    expect(policy).toEqual(CLOSED_POLICY);
  });

  it("rejects a host payload that smuggles an extra key", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      ...CLOSED_POLICY,
      nativePath: "C:\\smuggled",
    });
    const gateway = new TauriSettingsGateway();
    await expect(gateway.getPermissionPolicy("project-1")).rejects.toMatchObject({
      code: "invalid_settings_response",
    });
  });

  it("fail-closes effective bypass when the host claims capability is off", () => {
    expect(
      parsePermissionPolicy({
        bypassAvailable: false,
        bypassAvailableSource: "global",
        projectMode: "bypass",
        projectModeSource: "project",
        effectiveMode: "bypass",
      }).effectiveMode,
    ).toBe("default");
  });

  it("surfaces a missing host command instead of inventing a policy", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(
      new Error("command get_permission_policy not found"),
    );
    const gateway = new TauriSettingsGateway();
    await expect(gateway.getPermissionPolicy("project-1")).rejects.toMatchObject({
      code: "permission_policy_host_missing",
    });
    expect(vi.mocked(invoke).mock.calls.map(([name]) => name)).toEqual([
      "get_permission_policy",
    ]);
  });

  it("invokes set_bypass_available and set_project_permission_mode with closed parse", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({ ...CLOSED_POLICY, bypassAvailable: true, effectiveMode: "bypass" })
      .mockResolvedValueOnce({
        bypassAvailable: true,
        bypassAvailableSource: "global",
        projectMode: "bypass",
        projectModeSource: "project",
        effectiveMode: "bypass",
      });
    const gateway = new TauriSettingsGateway();
    await gateway.setBypassAvailable(true);
    await gateway.setProjectPermissionMode("project-1", "bypass");
    expect(invoke).toHaveBeenNthCalledWith(1, "set_bypass_available", {
      available: true,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "set_project_permission_mode", {
      projectId: "project-1",
      mode: "bypass",
    });
  });
});
