import { invoke } from "@tauri-apps/api/core";
import {
  SettingsGatewayError,
  type RuntimeAdapterLabel,
  type RuntimeSettingsView,
  type SettingSource,
  type SettingsGateway,
  type SettingsScope,
} from "./settingsTypes";

const INVALID_RESPONSE = "Pillow received an invalid settings response.";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNonEmptyString(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function isSettingSource(value: unknown): value is SettingSource {
  return (
    value === "env"
    || value === "project"
    || value === "global"
    || value === "default"
  );
}

function parseResolvedSetting(
  value: unknown,
  field: string,
): RuntimeSettingsView["adapter"] {
  if (
    !isRecord(value)
    || !isSettingSource(value.source)
    || (value.value !== null && typeof value.value !== "string")
  ) {
    throw new SettingsGatewayError(
      "invalid_settings_response",
      `${INVALID_RESPONSE} (${field})`,
    );
  }
  return {
    value: value.value === null ? null : value.value,
    source: value.source,
    envVar:
      typeof value.envVar === "string"
        ? value.envVar
        : value.envVar === null || value.envVar === undefined
        ? null
        : null,
  };
}

/**
 * Explicit structural guard: reject any secret-shaped keys on the wire DTO.
 * S0 has none; this keeps S1 from accidentally shipping secrets to the renderer.
 */
export function assertNoSecretFields(payload: unknown): void {
  const forbidden = [
    "secret",
    "token",
    "password",
    "credential",
    "apikey",
    "api_key",
    "auth",
    "privatekey",
    "private_key",
    "bearer",
    "oauth",
  ];
  const walk = (node: unknown, path: string): void => {
    if (Array.isArray(node)) {
      node.forEach((item, i) => walk(item, `${path}[${i}]`));
      return;
    }
    if (!isRecord(node)) return;
    for (const [key, child] of Object.entries(node)) {
      const lower = key.toLowerCase();
      for (const needle of forbidden) {
        if (lower.includes(needle)) {
          throw new SettingsGatewayError(
            "invalid_settings_response",
            `Settings response must not include secret-shaped field \`${path}${key}\`.`,
          );
        }
      }
      walk(child, `${path}${key}.`);
    }
  };
  walk(payload, "");
}

function parseRuntimeSettings(value: unknown): RuntimeSettingsView {
  assertNoSecretFields(value);
  if (
    !isRecord(value)
    || (value.projectId !== null
      && value.projectId !== undefined
      && typeof value.projectId !== "string")
  ) {
    throw new SettingsGatewayError("invalid_settings_response", INVALID_RESPONSE);
  }
  return {
    adapter: parseResolvedSetting(value.adapter, "adapter"),
    grokExecutable: parseResolvedSetting(value.grokExecutable, "grokExecutable"),
    grokProfile: parseResolvedSetting(value.grokProfile, "grokProfile"),
    projectId:
      typeof value.projectId === "string" && value.projectId.trim().length > 0
        ? value.projectId
        : null,
  };
}

function normalizeSettingsError(error: unknown): SettingsGatewayError {
  if (error instanceof SettingsGatewayError) return error;
  if (
    isRecord(error)
    && isNonEmptyString(error.code)
    && typeof error.message === "string"
    && error.message.trim().length > 0
  ) {
    return new SettingsGatewayError(error.code, error.message);
  }
  return new SettingsGatewayError(
    "settings_command_failed",
    "Settings command failed.",
  );
}

async function invokeSettings(
  command: string,
  args?: Record<string, unknown>,
): Promise<RuntimeSettingsView> {
  try {
    const payload = args
      ? await invoke<unknown>(command, args)
      : await invoke<unknown>(command);
    return parseRuntimeSettings(payload);
  } catch (error) {
    throw normalizeSettingsError(error);
  }
}

/** Tauri settings gateway — host owns storage; renderer never touches the DB. */
export class TauriSettingsGateway implements SettingsGateway {
  getRuntimeSettings(projectId?: string | null): Promise<RuntimeSettingsView> {
    return invokeSettings("get_runtime_settings", {
      projectId: projectId ?? null,
    });
  }

  setRuntimeAdapter(
    scope: SettingsScope,
    projectId: string | null,
    adapter: RuntimeAdapterLabel,
  ): Promise<RuntimeSettingsView> {
    return invokeSettings("set_runtime_adapter", {
      scope,
      projectId,
      adapter,
    });
  }

  setGrokExecutable(
    scope: SettingsScope,
    projectId: string | null,
    path: string,
  ): Promise<RuntimeSettingsView> {
    return invokeSettings("set_grok_executable", {
      scope,
      projectId,
      path,
    });
  }

  setGrokProfile(
    scope: SettingsScope,
    projectId: string | null,
    path: string,
  ): Promise<RuntimeSettingsView> {
    return invokeSettings("set_grok_profile", {
      scope,
      projectId,
      path,
    });
  }

  clearRuntimeSetting(
    scope: SettingsScope,
    projectId: string | null,
    key: "runtime.adapter" | "runtime.grok.executable" | "runtime.grok.profile",
  ): Promise<RuntimeSettingsView> {
    return invokeSettings("clear_runtime_setting", {
      scope,
      projectId,
      key,
    });
  }
}

/** In-memory gateway for unit tests (no Tauri). */
export class MockSettingsGateway implements SettingsGateway {
  private state: RuntimeSettingsView;

  constructor(initial?: Partial<RuntimeSettingsView>) {
    this.state = {
      adapter: {
        value: "grok-acp",
        source: "default",
        envVar: null,
      },
      grokExecutable: { value: null, source: "default", envVar: null },
      grokProfile: { value: null, source: "default", envVar: null },
      projectId: null,
      ...initial,
    };
  }

  /** Test seam: force env override presentation. */
  setEnvOverride(
    field: "adapter" | "grokExecutable" | "grokProfile",
    value: string,
    envVar: string,
  ): void {
    this.state = {
      ...this.state,
      [field]: { value, source: "env", envVar },
    };
  }

  getSnapshot(): RuntimeSettingsView {
    return structuredClone(this.state);
  }

  async getRuntimeSettings(
    projectId?: string | null,
  ): Promise<RuntimeSettingsView> {
    return {
      ...structuredClone(this.state),
      projectId: projectId ?? this.state.projectId,
    };
  }

  async setRuntimeAdapter(
    _scope: SettingsScope,
    projectId: string | null,
    adapter: RuntimeAdapterLabel,
  ): Promise<RuntimeSettingsView> {
    if (this.state.adapter.source === "env") {
      // Env still wins for effective view.
      return this.getRuntimeSettings(projectId);
    }
    this.state = {
      ...this.state,
      adapter: { value: adapter, source: projectId ? "project" : "global" },
      projectId,
    };
    return this.getRuntimeSettings(projectId);
  }

  async setGrokExecutable(
    _scope: SettingsScope,
    projectId: string | null,
    path: string,
  ): Promise<RuntimeSettingsView> {
    if (this.state.grokExecutable.source === "env") {
      return this.getRuntimeSettings(projectId);
    }
    this.state = {
      ...this.state,
      grokExecutable: {
        value: path,
        source: projectId ? "project" : "global",
      },
      projectId,
    };
    return this.getRuntimeSettings(projectId);
  }

  async setGrokProfile(
    _scope: SettingsScope,
    projectId: string | null,
    path: string,
  ): Promise<RuntimeSettingsView> {
    if (this.state.grokProfile.source === "env") {
      return this.getRuntimeSettings(projectId);
    }
    this.state = {
      ...this.state,
      grokProfile: {
        value: path,
        source: projectId ? "project" : "global",
      },
      projectId,
    };
    return this.getRuntimeSettings(projectId);
  }

  async clearRuntimeSetting(
    _scope: SettingsScope,
    projectId: string | null,
    key: "runtime.adapter" | "runtime.grok.executable" | "runtime.grok.profile",
  ): Promise<RuntimeSettingsView> {
    if (key === "runtime.adapter" && this.state.adapter.source !== "env") {
      this.state = {
        ...this.state,
        adapter: { value: "grok-acp", source: "default" },
      };
    }
    if (
      key === "runtime.grok.executable"
      && this.state.grokExecutable.source !== "env"
    ) {
      this.state = {
        ...this.state,
        grokExecutable: { value: null, source: "default" },
      };
    }
    if (
      key === "runtime.grok.profile"
      && this.state.grokProfile.source !== "env"
    ) {
      this.state = {
        ...this.state,
        grokProfile: { value: null, source: "default" },
      };
    }
    return this.getRuntimeSettings(projectId);
  }
}
