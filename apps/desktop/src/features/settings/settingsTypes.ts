/** S0 setting source attribution (matches host SettingSource). */
export type SettingSource = "env" | "project" | "global" | "default";

/** Constrained runtime adapter labels (matches RuntimeAdapterSetting). */
export type RuntimeAdapterLabel = "grok-acp" | "fake";

export type SettingsScope = "global" | "project";

export type ResolvedSettingView = {
  value: string | null;
  source: SettingSource;
  /** Present when source is env — allow-listed env var name. */
  envVar?: string | null;
};

/**
 * Renderer-facing runtime settings. S0 has no secret fields — keep this type
 * free of token/password/credential/auth keys forever.
 */
export type RuntimeSettingsView = {
  adapter: ResolvedSettingView;
  grokExecutable: ResolvedSettingView;
  grokProfile: ResolvedSettingView;
  projectId: string | null;
};

export type SettingsGatewayErrorCode =
  | "invalid_runtime_selection"
  | "settings_invalid_scope"
  | "settings_scope_shape"
  | "settings_unknown_key"
  | "settings_invalid_path"
  | "settings_invalid_value_kind"
  | "settings_invalid_value_json"
  | "settings_sqlite"
  | "settings_lock_poisoned"
  | "project_not_found"
  | "invalid_settings_response"
  | "settings_command_failed";

export type SettingsCommandError = {
  code: SettingsGatewayErrorCode | string;
  message: string;
};

export class SettingsGatewayError extends Error {
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "SettingsGatewayError";
    this.code = code;
  }
}

export interface SettingsGateway {
  getRuntimeSettings(projectId?: string | null): Promise<RuntimeSettingsView>;
  setRuntimeAdapter(
    scope: SettingsScope,
    projectId: string | null,
    adapter: RuntimeAdapterLabel,
  ): Promise<RuntimeSettingsView>;
  setGrokExecutable(
    scope: SettingsScope,
    projectId: string | null,
    path: string,
  ): Promise<RuntimeSettingsView>;
  setGrokProfile(
    scope: SettingsScope,
    projectId: string | null,
    path: string,
  ): Promise<RuntimeSettingsView>;
  clearRuntimeSetting(
    scope: SettingsScope,
    projectId: string | null,
    key: "runtime.adapter" | "runtime.grok.executable" | "runtime.grok.profile",
  ): Promise<RuntimeSettingsView>;
}

export function isEnvOverride(setting: ResolvedSettingView): boolean {
  return setting.source === "env";
}

export function sourceLabel(source: SettingSource): string {
  switch (source) {
    case "env":
      return "环境变量";
    case "project":
      return "项目";
    case "global":
      return "全局";
    case "default":
      return "默认";
    default:
      return source;
  }
}
