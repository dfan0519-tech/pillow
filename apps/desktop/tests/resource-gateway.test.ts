import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  ResourceGatewayError,
  type ResourceEntry,
  type ResourceFile,
  type RecentResource,
  type ResourceSearchResult,
} from "../src/lib/resourceGateway";
import { TauriResourceGateway } from "../src/lib/tauriResourceGateway";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const projectId = "00000000-0000-0000-0000-0000000000aa";
const otherProjectId = "00000000-0000-0000-0000-0000000000bb";

const entry: ResourceEntry = {
  path: "src/main.rs",
  kind: "file",
  sizeBytes: 128,
};

const directoryEntry: ResourceEntry = {
  path: "src",
  kind: "directory",
};

const file: ResourceFile = {
  path: "src/main.rs",
  contentKind: "utf8",
  content: "fn main() {}",
  bytesReturned: 12,
  truncated: false,
};

const binaryFile: ResourceFile = {
  path: "assets/icon.png",
  contentKind: "binary",
  bytesReturned: 64,
  truncated: false,
};

const searchHit: ResourceSearchResult = {
  path: "src/main.rs",
  line: 1,
  column: 4,
  preview: "fn main() {}",
};

const recent: RecentResource = {
  path: "src/main.rs",
  openedAt: "2026-07-31T00:00:00Z",
};

describe("TauriResourceGateway contract", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it("requires projectId on every list/read/search/recent operation", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce([entry])
      .mockResolvedValueOnce(file)
      .mockResolvedValueOnce([searchHit])
      .mockResolvedValueOnce([recent]);
    const gateway = new TauriResourceGateway();

    await gateway.listProjectResources({ projectId });
    await gateway.readProjectResource({ projectId, relativePath: "src/main.rs" });
    await gateway.searchProjectResources({ projectId, query: "main" });
    await gateway.listRecentProjectResources({ projectId });

    expect(invoke).toHaveBeenNthCalledWith(1, "list_project_resources", {
      projectId,
      relativePath: undefined,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "read_project_resource", {
      projectId,
      relativePath: "src/main.rs",
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "search_project_resources", {
      projectId,
      query: "main",
      relativePath: undefined,
    });
    expect(invoke).toHaveBeenNthCalledWith(4, "list_recent_project_resources", {
      projectId,
    });
  });

  it("forwards optional relativePath for list and search", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce([directoryEntry])
      .mockResolvedValueOnce([searchHit]);
    const gateway = new TauriResourceGateway();

    await gateway.listProjectResources({ projectId, relativePath: "src" });
    await gateway.searchProjectResources({
      projectId,
      query: "main",
      relativePath: "src",
    });

    expect(invoke).toHaveBeenNthCalledWith(1, "list_project_resources", {
      projectId,
      relativePath: "src",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "search_project_resources", {
      projectId,
      query: "main",
      relativePath: "src",
    });
  });

  it("accepts camelCase list, file, search, and recent DTOs", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce([entry, directoryEntry])
      .mockResolvedValueOnce(file)
      .mockResolvedValueOnce([searchHit])
      .mockResolvedValueOnce([recent]);
    const gateway = new TauriResourceGateway();

    await expect(gateway.listProjectResources({ projectId })).resolves.toEqual([
      entry,
      directoryEntry,
    ]);
    await expect(
      gateway.readProjectResource({ projectId, relativePath: "src/main.rs" }),
    ).resolves.toEqual(file);
    await expect(
      gateway.searchProjectResources({ projectId, query: "main" }),
    ).resolves.toEqual([searchHit]);
    await expect(
      gateway.listRecentProjectResources({ projectId }),
    ).resolves.toEqual([recent]);
  });

  it("accepts binary preview metadata without raw bytes", async () => {
    vi.mocked(invoke).mockResolvedValue(binaryFile);
    const gateway = new TauriResourceGateway();

    const result = await gateway.readProjectResource({
      projectId,
      relativePath: "assets/icon.png",
    });

    expect(result).toEqual(binaryFile);
    expect(result).not.toHaveProperty("bytes");
    expect(result.content).toBeUndefined();
  });

  it("rejects binary payloads that smuggle raw bytes to the renderer", async () => {
    vi.mocked(invoke).mockResolvedValue({
      ...binaryFile,
      bytes: [0, 1, 2, 3],
    });
    const gateway = new TauriResourceGateway();

    await expect(
      gateway.readProjectResource({ projectId, relativePath: "assets/icon.png" }),
    ).rejects.toMatchObject({
      name: "ResourceGatewayError",
      code: "invalid_resource_response",
    });
  });

  it.each([
    "path",
    "kind",
  ] as const)("rejects a resource entry missing %s", async (field) => {
    const malformed: Partial<ResourceEntry> = { ...entry };
    delete malformed[field];
    vi.mocked(invoke).mockResolvedValue([malformed]);
    const gateway = new TauriResourceGateway();

    await expect(gateway.listProjectResources({ projectId })).rejects.toMatchObject({
      name: "ResourceGatewayError",
      code: "invalid_resource_response",
      message: "Pillow received an invalid resource response.",
    });
  });

  it.each([
    "path",
    "contentKind",
    "bytesReturned",
    "truncated",
  ] as const)("rejects a resource file missing %s", async (field) => {
    const malformed: Partial<ResourceFile> = { ...file };
    delete malformed[field];
    vi.mocked(invoke).mockResolvedValue(malformed);
    const gateway = new TauriResourceGateway();

    await expect(
      gateway.readProjectResource({ projectId, relativePath: "src/main.rs" }),
    ).rejects.toMatchObject({
      code: "invalid_resource_response",
    });
  });

  it("rejects absolute or machine paths in list/read/search/recent payloads", async () => {
    const gateway = new TauriResourceGateway();

    vi.mocked(invoke).mockResolvedValueOnce([
      { ...entry, path: "<repo>\\\\src\\\\main.rs" },
    ]);
    await expect(gateway.listProjectResources({ projectId })).rejects.toMatchObject({
      code: "invalid_resource_response",
    });

    vi.mocked(invoke).mockResolvedValueOnce({
      ...file,
      path: "/absolute/main.rs",
    });
    await expect(
      gateway.readProjectResource({ projectId, relativePath: "src/main.rs" }),
    ).rejects.toMatchObject({
      code: "invalid_resource_response",
    });

    vi.mocked(invoke).mockResolvedValueOnce([
      { ...searchHit, path: "C:/Windows/system32" },
    ]);
    await expect(
      gateway.searchProjectResources({ projectId, query: "x" }),
    ).rejects.toMatchObject({
      code: "invalid_resource_response",
    });

    vi.mocked(invoke).mockResolvedValueOnce([
      { ...recent, path: "\\\\server\\share\\file" },
    ]);
    await expect(
      gateway.listRecentProjectResources({ projectId }),
    ).rejects.toMatchObject({
      code: "invalid_resource_response",
    });
  });

  it("rejects payloads that embed a project root or native handle", async () => {
    vi.mocked(invoke).mockResolvedValue({
      ...file,
      projectRoot: "<repo>",
      nativeHandle: 42,
    });
    const gateway = new TauriResourceGateway();

    await expect(
      gateway.readProjectResource({ projectId, relativePath: "src/main.rs" }),
    ).rejects.toMatchObject({
      code: "invalid_resource_response",
    });
  });

  it("preserves allow-listed opaque command errors", async () => {
    vi.mocked(invoke).mockRejectedValue({
      code: "project_not_found",
      message: "Project not found.",
    });
    const gateway = new TauriResourceGateway();

    await expect(
      gateway.listProjectResources({ projectId: otherProjectId }),
    ).rejects.toMatchObject({
      name: "ResourceGatewayError",
      code: "project_not_found",
      message: "Project not found.",
    });
  });

  it("preserves the opaque blank-search rejection without trusting hostile errors", async () => {
    const gateway = new TauriResourceGateway();

    vi.mocked(invoke).mockRejectedValueOnce({
      code: "resource_search_query_rejected",
      message: "Resource search query was rejected.",
    });
    await expect(
      gateway.searchProjectResources({ projectId, query: " " }),
    ).rejects.toMatchObject({
      name: "ResourceGatewayError",
      code: "resource_search_query_rejected",
      message: "Resource search query was rejected.",
    });

    vi.mocked(invoke).mockRejectedValueOnce({
      code: "unknown_resource_error",
      message: "F:\\\\secret\\\\keeper.db",
    });
    await expect(
      gateway.searchProjectResources({ projectId, query: " " }),
    ).rejects.toMatchObject({
      name: "ResourceGatewayError",
      code: "resource_command_failed",
      message: "Resource command failed.",
    });

    vi.mocked(invoke).mockRejectedValueOnce({
      code: "resource_search_query_rejected",
      message: "F:\\\\secret\\\\keeper.db",
    });
    await expect(
      gateway.searchProjectResources({ projectId, query: " " }),
    ).rejects.toMatchObject({
      name: "ResourceGatewayError",
      code: "resource_command_failed",
      message: "Resource command failed.",
    });
  });

  it.each([
    new Error("database path F:\\\\secret\\\\keeper.db leaked"),
    { code: "sqlite_error", message: "open F:\\\\secret\\\\keeper.db" },
    { code: "project_not_found" },
    "unexpected rejection with F:\\\\path",
    { code: "resource_path_rejected", message: "  " },
  ])("normalizes unknown or path-bearing errors to opaque failures", async (payload) => {
    vi.mocked(invoke).mockRejectedValue(payload);
    const gateway = new TauriResourceGateway();

    await expect(gateway.listProjectResources({ projectId })).rejects.toMatchObject({
      name: "ResourceGatewayError",
      code: "resource_command_failed",
      message: "Resource command failed.",
    });
  });

  it("rejects non-array list and search payloads", async () => {
    const gateway = new TauriResourceGateway();
    vi.mocked(invoke).mockResolvedValueOnce(entry);
    await expect(gateway.listProjectResources({ projectId })).rejects.toBeInstanceOf(
      ResourceGatewayError,
    );

    vi.mocked(invoke).mockResolvedValueOnce(searchHit);
    await expect(
      gateway.searchProjectResources({ projectId, query: "main" }),
    ).rejects.toMatchObject({
      code: "invalid_resource_response",
    });
  });

  it("rejects invalid entry kind and contentKind enums", async () => {
    const gateway = new TauriResourceGateway();
    vi.mocked(invoke).mockResolvedValueOnce([{ ...entry, kind: "socket" }]);
    await expect(gateway.listProjectResources({ projectId })).rejects.toMatchObject({
      code: "invalid_resource_response",
    });

    vi.mocked(invoke).mockResolvedValueOnce({ ...file, contentKind: "image" });
    await expect(
      gateway.readProjectResource({ projectId, relativePath: "src/main.rs" }),
    ).rejects.toMatchObject({
      code: "invalid_resource_response",
    });
  });
});
