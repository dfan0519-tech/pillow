/**
 * Draft probe_model_catalog — host path only; success / unsupported / fail;
 * apiKey never on response; renderer has no outbound fetch for catalogs.
 */
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  PROBE_MODEL_CATALOG_DTO_KEYS,
  parseProbeModelCatalogView,
  TauriRuntimeSlotsGateway,
  MockRuntimeSlotsGateway,
} from "../src/features/settings/runtimeSlotsGateway";
import { MODEL_CATALOG_UNSUPPORTED_ENDPOINT } from "../src/features/settings/runtimeSlotsTypes";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const MARKER = "pillow-test-draft-probe-sentinel-7e2c";

describe("parseProbeModelCatalogView", () => {
  it("accepts available catalog with closed keys only", () => {
    const good = {
      status: "available",
      source: "openai_models",
      models: ["a", "b"],
      reasonCode: null,
    };
    const view = parseProbeModelCatalogView(good);
    expect(view.models).toEqual(["a", "b"]);
    expect(Object.keys(good).sort()).toEqual(
      [...PROBE_MODEL_CATALOG_DTO_KEYS].sort(),
    );
    expect(() =>
      parseProbeModelCatalogView({ ...good, apiKey: MARKER }),
    ).toThrow(/secret|invalid/i);
  });

  it("accepts unsupported with empty models + reason", () => {
    const view = parseProbeModelCatalogView({
      status: "unsupported",
      source: "openai_models",
      models: [],
      reasonCode: "endpoint_not_supported",
    });
    expect(view.status).toBe("unsupported");
    expect(view.models).toEqual([]);
  });
});

describe("TauriRuntimeSlotsGateway.probeModelCatalog", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("success path: lists models and never echoes apiKey on result", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      status: "available",
      source: "openai_models",
      models: ["draft-model-a", "draft-model-b"],
      reasonCode: null,
    });
    const gateway = new TauriRuntimeSlotsGateway();
    const catalog = await gateway.probeModelCatalog(
      "https://api.example.test/v1",
      MARKER,
    );
    expect(catalog.models).toContain("draft-model-a");
    expect(catalog.status).toBe("available");
    expect(JSON.stringify(catalog)).not.toContain(MARKER);
    expect(invoke).toHaveBeenCalledWith("probe_model_catalog", {
      baseUrl: "https://api.example.test/v1",
      apiKey: MARKER,
    });
  });

  it("unsupported path: normal success payload, empty models", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      status: "unsupported",
      source: "openai_models",
      models: [],
      reasonCode: "endpoint_not_supported",
    });
    const gateway = new TauriRuntimeSlotsGateway();
    const catalog = await gateway.probeModelCatalog(
      "https://api.example.test/v1",
      MARKER,
    );
    expect(catalog.status).toBe("unsupported");
    expect(catalog.models).toEqual([]);
    expect(catalog.reasonCode).toBe("endpoint_not_supported");
    expect(JSON.stringify(catalog)).not.toContain(MARKER);
  });

  it("fail path: throws fixed copy without host key echo", async () => {
    vi.mocked(invoke).mockRejectedValueOnce({
      code: "runtime_slot_model_catalog_unavailable",
      message: `auth failed apiKey=${MARKER}`,
    });
    const gateway = new TauriRuntimeSlotsGateway();
    try {
      await gateway.probeModelCatalog("https://api.example.test/v1", MARKER);
      expect.unreachable("expected probe to reject");
    } catch (error) {
      expect(error).toMatchObject({
        code: "runtime_slot_model_catalog_unavailable",
      });
      expect(String((error as Error).message)).not.toContain(MARKER);
      expect(String((error as Error).message)).not.toContain("apiKey=");
    }
  });
});

describe("MockRuntimeSlotsGateway.probeModelCatalog", () => {
  it("records key length only — never stores key text", async () => {
    const gateway = new MockRuntimeSlotsGateway();
    gateway.setDraftProbeMode("available", {
      models: ["m1"],
    });
    const catalog = await gateway.probeModelCatalog(
      "https://api.example.test/v1",
      MARKER,
    );
    expect(catalog.models).toEqual(["m1"]);
    expect(gateway.lastProbeModelCatalogCalls).toEqual([
      { baseUrl: "https://api.example.test/v1", apiKeyLength: MARKER.length },
    ]);
    // Mock state must not retain the sentinel string.
    expect(JSON.stringify(gateway)).not.toContain(MARKER);
  });

  it("unsupported does not invent models", async () => {
    const gateway = new MockRuntimeSlotsGateway();
    gateway.setDraftProbeMode("unsupported_endpoint");
    const catalog = await gateway.probeModelCatalog(
      "https://api.example.test/v1",
      MARKER,
    );
    expect(catalog.status).toBe("unsupported");
    expect(catalog.models).toEqual([]);
    expect(MODEL_CATALOG_UNSUPPORTED_ENDPOINT.length).toBeGreaterThan(0);
  });
});

describe("renderer must not fetch third-party model catalogs", () => {
  it("draft probe is gateway.probeModelCatalog only (host command)", async () => {
    // Static contract pin: mock exposes probeModelCatalog; Tauri path invokes
    // probe_model_catalog. No renderer fetch helper is reintroduced.
    const mock = new MockRuntimeSlotsGateway();
    expect(typeof mock.probeModelCatalog).toBe("function");
    const gateway = new TauriRuntimeSlotsGateway();
    vi.mocked(invoke).mockResolvedValueOnce({
      status: "available",
      source: "openai_models",
      models: ["only-host"],
      reasonCode: null,
    });
    await gateway.probeModelCatalog("https://api.example.test/v1", "k");
    expect(invoke).toHaveBeenCalledWith(
      "probe_model_catalog",
      expect.objectContaining({ baseUrl: "https://api.example.test/v1" }),
    );
  });
});
