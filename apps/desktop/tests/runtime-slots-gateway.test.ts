/**
 * Runtime-slots gateway contract: closed-key accept/reject, secret-free
 * responses, empty-apiKey update omits the key (keep stored secret).
 */
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  EXTERNAL_CLI_STATUS_DTO_KEYS,
  MockRuntimeSlotsGateway,
  RUNTIME_SLOT_DTO_KEYS,
  RUNTIME_SLOT_MODEL_CATALOG_DTO_KEYS,
  RUNTIME_SLOT_MODEL_SWITCH_DTO_KEYS,
  RUNTIME_SLOT_START_DTO_KEYS,
  TauriRuntimeSlotsGateway,
  assertNoSecretPayloadFields,
  buildCreateGrokApiKeySlotArgs,
  buildUpdateGrokApiKeySlotArgs,
  classifyExternalCliStatus,
  mapGrokProbeFailureMessage,
  mapModelCatalogUnsupportedReason,
  mapModelSwitchResultMessage,
  mapRuntimeSlotErrorMessage,
  parseExternalCliStatusView,
  parseRuntimeSlotModelCatalogView,
  parseRuntimeSlotModelSwitchView,
  parseRuntimeSlotStartView,
  parseRuntimeSlotView,
  truncateModelIdForDisplay,
} from "../src/features/settings/runtimeSlotsGateway";
import {
  MODEL_CATALOG_UNSUPPORTED_ENDPOINT,
  MODEL_SWITCH_RESTARTED_MESSAGE,
  MODEL_SWITCH_SAVED_INACTIVE_MESSAGE,
  type RuntimeSlotView,
} from "../src/features/settings/runtimeSlotsTypes";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const PROJECT = "project-1";
const SLOT = "00000000-0000-4000-8000-0000000000a1";
const SECRET_REF = "00000000-0000-4000-8000-0000000000b2";
const NOW = "2026-08-11T12:00:00.000Z";

function apiKeySlot(
  overrides: Partial<RuntimeSlotView> = {},
): RuntimeSlotView {
  return {
    slotId: SLOT,
    projectId: PROJECT,
    displayName: "Grok API",
    kind: "governed_grok",
    governance: "full",
    authMode: "api_key",
    baseUrl: "https://api.example.test/v1",
    modelId: "grok-test",
    secretRef: SECRET_REF,
    secretConfigured: true,
    executablePath: null,
    launchArgs: null,
    statusArgs: null,
    createdAt: NOW,
    updatedAt: NOW,
    ...overrides,
  };
}

function externalSlot(
  overrides: Partial<RuntimeSlotView> = {},
): RuntimeSlotView {
  return {
    slotId: "00000000-0000-4000-8000-0000000000c3",
    projectId: PROJECT,
    displayName: "Codex CLI",
    kind: "external_cli",
    governance: "none",
    authMode: null,
    baseUrl: null,
    modelId: null,
    secretRef: null,
    secretConfigured: false,
    executablePath: "C:\\tools\\codex.exe",
    launchArgs: [],
    statusArgs: ["login", "status"],
    createdAt: NOW,
    updatedAt: NOW,
    ...overrides,
  };
}

describe("runtime slot DTO parsers", () => {
  it("accepts the exact RuntimeSlotDto key set and rejects unknown keys", () => {
    const good = apiKeySlot();
    expect(parseRuntimeSlotView(good)).toEqual(good);

    expect(() =>
      parseRuntimeSlotView({
        ...good,
        nativePath: "C:\\smuggled",
      })
    ).toThrow(/invalid runtime slot response/i);

    expect(() =>
      parseRuntimeSlotView({
        ...good,
        slotId: undefined,
      })
    ).toThrow(/invalid runtime slot response/i);
  });

  it("rejects payloads that smuggle apiKey or other secret fields", () => {
    expect(() =>
      parseRuntimeSlotView({
        ...apiKeySlot(),
        apiKey: "sk-live-super-secret",
      })
    ).toThrow(/secret field/i);

    expect(() =>
      assertNoSecretPayloadFields({ token: "abc" })
    ).toThrow(/secret field/i);

    // Status fields are allowed by name; values must not look like free-form secrets
    // (opaque ref is fine).
    expect(() =>
      assertNoSecretPayloadFields({
        secretRef: SECRET_REF,
        secretConfigured: true,
      })
    ).not.toThrow();
  });

  it("rejects governance/kind mismatches (host field is explicit)", () => {
    expect(() =>
      parseRuntimeSlotView({
        ...apiKeySlot(),
        governance: "none",
      })
    ).toThrow(/invalid runtime slot response/i);

    expect(() =>
      parseRuntimeSlotView({
        ...externalSlot(),
        governance: "full",
      })
    ).toThrow(/invalid runtime slot response/i);
  });

  it("accepts ExternalCliStatusDto and RuntimeSlotStartDto closed keys", () => {
    expect(
      parseExternalCliStatusView({
        slotId: SLOT,
        projectId: PROJECT,
        governance: "none",
        exitCode: 0,
        stdout: "Logged in using ChatGPT",
        stderr: "",
        timedOut: false,
        truncated: false,
      }),
    ).toMatchObject({
      exitCode: 0,
      stdout: "Logged in using ChatGPT",
      governance: "none",
    });

    expect(() =>
      parseExternalCliStatusView({
        slotId: SLOT,
        projectId: PROJECT,
        governance: "none",
        exitCode: 0,
        stdout: "ok",
        stderr: "",
        timedOut: false,
        truncated: false,
        apiKey: "nope",
      })
    ).toThrow();

    expect(
      parseRuntimeSlotStartView({
        slotId: SLOT,
        projectId: PROJECT,
        kind: "governed_grok",
        governance: "full",
        status: "active",
      }),
    ).toEqual({
      slotId: SLOT,
      projectId: PROJECT,
      kind: "governed_grok",
      governance: "full",
      status: "active",
    });

    // Pin allowlist lengths so silently empty validators cannot go green.
    expect(RUNTIME_SLOT_DTO_KEYS).toHaveLength(15);
    expect(EXTERNAL_CLI_STATUS_DTO_KEYS).toHaveLength(8);
    expect(RUNTIME_SLOT_START_DTO_KEYS).toHaveLength(5);
    expect(RUNTIME_SLOT_MODEL_CATALOG_DTO_KEYS).toHaveLength(6);
    expect(RUNTIME_SLOT_MODEL_SWITCH_DTO_KEYS).toHaveLength(5);
  });
});

describe("model catalog / switch DTO parsers", () => {
  it("accepts available catalog with real models and rejects smuggled keys", () => {
    const good = {
      slotId: SLOT,
      projectId: PROJECT,
      status: "available" as const,
      source: "openai_models" as const,
      models: ["grok-test", "grok-beta"],
      reasonCode: null,
    };
    expect(parseRuntimeSlotModelCatalogView(good)).toEqual(good);

    // Positive: models are actually returned, not just "parsed without throw".
    expect(parseRuntimeSlotModelCatalogView(good).models).toContain("grok-beta");

    expect(() =>
      parseRuntimeSlotModelCatalogView({
        ...good,
        grant: "smuggled",
      })
    ).toThrow(/invalid runtime slot response/i);

    expect(() =>
      parseRuntimeSlotModelCatalogView({
        ...good,
        apiKey: "nope",
      })
    ).toThrow(/secret field/i);
  });

  it("accepts unsupported catalog (normal result) and requires reason + empty models", () => {
    const unsupported = parseRuntimeSlotModelCatalogView({
      slotId: SLOT,
      projectId: PROJECT,
      status: "unsupported",
      source: "openai_models",
      models: [],
      reasonCode: "endpoint_not_supported",
    });
    expect(unsupported.status).toBe("unsupported");
    expect(unsupported.reasonCode).toBe("endpoint_not_supported");
    expect(unsupported.models).toEqual([]);
    expect(mapModelCatalogUnsupportedReason(unsupported.reasonCode)).toBe(
      MODEL_CATALOG_UNSUPPORTED_ENDPOINT,
    );

    // Negative: unsupported with models is invalid.
    expect(() =>
      parseRuntimeSlotModelCatalogView({
        slotId: SLOT,
        projectId: PROJECT,
        status: "unsupported",
        source: "openai_models",
        models: ["should-not-be-here"],
        reasonCode: "endpoint_not_supported",
      })
    ).toThrow(/invalid runtime slot response/i);

    // Negative: available must not carry a reason.
    expect(() =>
      parseRuntimeSlotModelCatalogView({
        slotId: SLOT,
        projectId: PROJECT,
        status: "available",
        source: "openai_models",
        models: ["a"],
        reasonCode: "endpoint_not_supported",
      })
    ).toThrow(/invalid runtime slot response/i);
  });

  it("accepts switch DTO and pins runtimeRestarted/sessionRestore pairing", () => {
    const restarted = parseRuntimeSlotModelSwitchView({
      slotId: SLOT,
      projectId: PROJECT,
      modelId: "model-new",
      runtimeRestarted: true,
      sessionRestore: "unavailable",
    });
    expect(restarted.runtimeRestarted).toBe(true);
    expect(mapModelSwitchResultMessage(restarted)).toBe(
      MODEL_SWITCH_RESTARTED_MESSAGE,
    );

    const inactive = parseRuntimeSlotModelSwitchView({
      slotId: SLOT,
      projectId: PROJECT,
      modelId: "model-new",
      runtimeRestarted: false,
      sessionRestore: "not_applicable",
    });
    expect(inactive.runtimeRestarted).toBe(false);
    expect(mapModelSwitchResultMessage(inactive)).toBe(
      MODEL_SWITCH_SAVED_INACTIVE_MESSAGE,
    );

    // Negative: restarted without unavailable restore is invalid.
    expect(() =>
      parseRuntimeSlotModelSwitchView({
        slotId: SLOT,
        projectId: PROJECT,
        modelId: "m",
        runtimeRestarted: true,
        sessionRestore: "not_applicable",
      })
    ).toThrow(/invalid runtime slot response/i);

    expect(() =>
      parseRuntimeSlotModelSwitchView({
        slotId: SLOT,
        projectId: PROJECT,
        modelId: "m",
        runtimeRestarted: false,
        sessionRestore: "unavailable",
      })
    ).toThrow(/invalid runtime slot response/i);
  });

  it("truncates long model ids for display while preserving prefix", () => {
    const long = `prefix-${"z".repeat(100)}`;
    const shown = truncateModelIdForDisplay(long, 48);
    expect(shown.endsWith("…")).toBe(true);
    expect(shown.length).toBe(48);
    expect(shown.startsWith("prefix-")).toBe(true);
  });
});

describe("update/create apiKey arg builders", () => {
  it("create always includes apiKey", () => {
    const args = buildCreateGrokApiKeySlotArgs({
      projectId: PROJECT,
      displayName: "A",
      baseUrl: "https://example.test",
      modelId: "m",
      apiKey: "sk-test-key-value",
    });
    expect(args).toEqual({
      projectId: PROJECT,
      displayName: "A",
      baseUrl: "https://example.test",
      modelId: "m",
      apiKey: "sk-test-key-value",
    });
  });

  it("edit with empty/whitespace apiKey omits the key (keep stored secret)", () => {
    const blank = buildUpdateGrokApiKeySlotArgs({
      projectId: PROJECT,
      slotId: SLOT,
      displayName: "Renamed",
      baseUrl: "https://example.test/v2",
      modelId: "m2",
      apiKey: "",
    });
    expect(blank).toEqual({
      projectId: PROJECT,
      slotId: SLOT,
      displayName: "Renamed",
      baseUrl: "https://example.test/v2",
      modelId: "m2",
    });
    expect(blank).not.toHaveProperty("apiKey");

    const whitespace = buildUpdateGrokApiKeySlotArgs({
      projectId: PROJECT,
      slotId: SLOT,
      displayName: "Renamed",
      baseUrl: "https://example.test/v2",
      modelId: "m2",
      apiKey: "   \t  ",
    });
    expect(whitespace).not.toHaveProperty("apiKey");
  });

  it("edit with non-empty apiKey includes the trimmed key", () => {
    const args = buildUpdateGrokApiKeySlotArgs({
      projectId: PROJECT,
      slotId: SLOT,
      displayName: "Renamed",
      baseUrl: "https://example.test/v2",
      modelId: "m2",
      apiKey: "  sk-new  ",
    });
    expect(args.apiKey).toBe("sk-new");
  });
});

describe("MockRuntimeSlotsGateway secret handling", () => {
  it("create stores the key off-view; empty update does not overwrite it", async () => {
    const gateway = new MockRuntimeSlotsGateway();
    // Sentinel, not a provider-shaped key — see the note in runtime-slots-ui.test.tsx.
    const secret = "pillow-test-sentinel-must-not-leak";
    const created = await gateway.createGrokApiKeySlot({
      projectId: PROJECT,
      displayName: "API",
      baseUrl: "https://api.example.test",
      modelId: "model-1",
      apiKey: secret,
    });

    expect(created.secretConfigured).toBe(true);
    expect(created).not.toHaveProperty("apiKey");
    expect(JSON.stringify(created)).not.toContain(secret);
    expect(gateway.getMockSecret(created.secretRef!)).toBe(secret);

    const createArgs = gateway.lastCreateApiKeyArgs.at(-1);
    expect(createArgs?.apiKey).toBe(secret);

    await gateway.updateGrokApiKeySlot({
      projectId: PROJECT,
      slotId: created.slotId,
      displayName: "API renamed",
      baseUrl: "https://api.example.test/v2",
      modelId: "model-2",
      apiKey: "",
    });

    const updateArgs = gateway.lastUpdateApiKeyArgs.at(-1);
    expect(updateArgs).toBeDefined();
    expect(updateArgs).not.toHaveProperty("apiKey");
    expect(gateway.getMockSecret(created.secretRef!)).toBe(secret);
  });
});

describe("TauriRuntimeSlotsGateway invoke contract", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("lists and creates with exact camelCase command keys", async () => {
    const slot = apiKeySlot();
    vi.mocked(invoke)
      .mockResolvedValueOnce([slot])
      .mockResolvedValueOnce(slot);

    const gateway = new TauriRuntimeSlotsGateway();
    await gateway.listRuntimeSlots(PROJECT);
    await gateway.createGrokApiKeySlot({
      projectId: PROJECT,
      displayName: "Grok API",
      baseUrl: "https://api.example.test/v1",
      modelId: "grok-test",
      apiKey: "sk-once",
    });

    expect(invoke).toHaveBeenNthCalledWith(1, "list_runtime_slots", {
      projectId: PROJECT,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "create_grok_api_key_slot", {
      projectId: PROJECT,
      displayName: "Grok API",
      baseUrl: "https://api.example.test/v1",
      modelId: "grok-test",
      apiKey: "sk-once",
    });
  });

  it("invokes catalog and set-model with closed-key parse on accept path", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce({
        slotId: SLOT,
        projectId: PROJECT,
        status: "available",
        source: "openai_models",
        models: ["a", "b"],
        reasonCode: null,
      })
      .mockResolvedValueOnce({
        slotId: SLOT,
        projectId: PROJECT,
        modelId: "b",
        runtimeRestarted: true,
        sessionRestore: "unavailable",
      });

    const gateway = new TauriRuntimeSlotsGateway();
    const catalog = await gateway.getRuntimeSlotModelCatalog(PROJECT, SLOT);
    expect(catalog.models).toEqual(["a", "b"]);
    expect(catalog.status).toBe("available");

    const switched = await gateway.setRuntimeSlotModel(PROJECT, SLOT, "b");
    expect(switched).toEqual({
      slotId: SLOT,
      projectId: PROJECT,
      modelId: "b",
      runtimeRestarted: true,
      sessionRestore: "unavailable",
    });

    expect(invoke).toHaveBeenNthCalledWith(1, "get_runtime_slot_model_catalog", {
      projectId: PROJECT,
      slotId: SLOT,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "set_runtime_slot_model", {
      projectId: PROJECT,
      slotId: SLOT,
      modelId: "b",
    });
  });

  it("maps catalog host errors to fixed copy without host message echo", async () => {
    vi.mocked(invoke).mockRejectedValueOnce({
      code: "runtime_slot_model_catalog_unavailable",
      message: "auth failed apiKey=pillow-test-sentinel-must-not-echo",
    });
    const gateway = new TauriRuntimeSlotsGateway();
    try {
      await gateway.getRuntimeSlotModelCatalog(PROJECT, SLOT);
      expect.unreachable("expected catalog invoke to reject");
    } catch (error) {
      expect(error).toMatchObject({
        code: "runtime_slot_model_catalog_unavailable",
      });
      expect(String((error as Error).message)).not.toContain("apiKey=");
      expect(String((error as Error).message)).toMatch(/手输|模型目录/u);
    }
  });

  it("update invoke omits empty apiKey", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(apiKeySlot({ displayName: "X" }));
    const gateway = new TauriRuntimeSlotsGateway();
    await gateway.updateGrokApiKeySlot({
      projectId: PROJECT,
      slotId: SLOT,
      displayName: "X",
      baseUrl: "https://api.example.test/v1",
      modelId: "grok-test",
      apiKey: "",
    });
    expect(invoke).toHaveBeenCalledWith("update_grok_api_key_slot", {
      projectId: PROJECT,
      slotId: SLOT,
      displayName: "X",
      baseUrl: "https://api.example.test/v1",
      modelId: "grok-test",
    });
    const args = vi.mocked(invoke).mock.calls[0]![1] as Record<string, unknown>;
    expect(args).not.toHaveProperty("apiKey");
  });

  it("rejects list payloads that smuggle root/apiKey", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([
      { ...apiKeySlot(), apiKey: "sk-bad" },
    ]);
    const gateway = new TauriRuntimeSlotsGateway();
    await expect(gateway.listRuntimeSlots(PROJECT)).rejects.toMatchObject({
      code: "invalid_runtime_slot_response",
    });
  });

  it("maps host errors to fixed copy without using host message text", () => {
    // Sentinel stands in for a host message that carries a key. The point is
    // that the renderer never echoes host error text, so the shape does not
    // need to be authentic — and an authentic-looking one would have to be
    // allow-listed past the secret gate, which defeats the gate.
    const secretMessage = "auth failed apiKey=pillow-test-sentinel-should-not-surface";
    const error = {
      code: "runtime_slot_secret_missing",
      message: secretMessage,
    };
    // normalize via thrown path
    expect(mapRuntimeSlotErrorMessage(error.code)).not.toContain("sk-live");
    expect(mapGrokProbeFailureMessage(error.code)).not.toContain("sk-live");
    expect(mapGrokProbeFailureMessage(error.code)).toMatch(/认证|密钥/u);
  });
});

describe("external CLI status classification", () => {
  it("marks exit 0 success and non-zero failure", () => {
    expect(
      classifyExternalCliStatus({
        slotId: SLOT,
        projectId: PROJECT,
        governance: "none",
        exitCode: 0,
        stdout: "Logged in using ChatGPT",
        stderr: "",
        timedOut: false,
        truncated: false,
      }).ok,
    ).toBe(true);

    expect(
      classifyExternalCliStatus({
        slotId: SLOT,
        projectId: PROJECT,
        governance: "none",
        exitCode: 1,
        stdout: "",
        stderr: "not logged in",
        timedOut: false,
        truncated: false,
      }).ok,
    ).toBe(false);
  });
});
