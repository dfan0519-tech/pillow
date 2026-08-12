/** S0 setting source attribution (matches host SettingSource). */
export type SettingSource = "env" | "project" | "global" | "default";

/** Constrained runtime adapter labels (matches RuntimeAdapterSetting). */
export type RuntimeAdapterLabel = "grok-acp" | "fake";

/** Constrained UI theme labels (matches UiThemeSetting). No `system`. */
export type UiThemeLabel = "dark" | "light";

/** Pillow-owned approval gate; never forwarded to a provider runtime. */
export type PermissionModeLabel = "default" | "accept_edits" | "bypass";

/**
 * Legacy installation-wide mode DTO (Rust `PermissionModeSettingsDto`).
 * Still used by the workbench indicator via `getPermissionMode` until the
 * host ships two-tier effective-mode resolution with a project context.
 */
export type PermissionModeSettingsView = {
  mode: PermissionModeLabel;
  source: "global" | "default";
};

/**
 * Two-tier permission policy (global Bypass capability ∧ project mode).
 * Closed-key host DTO — adding fields is a breaking change for the parser.
 *
 * effectiveMode = projectMode when projectMode !== "bypass"
 *               | "bypass" when bypassAvailable && projectMode === "bypass"
 *               | fail-closed fallback when project wants bypass but global denies.
 * When projectMode is null (no project), effectiveMode uses the host's
 * installation preferred mode with the same Bypass gate.
 */
export type PermissionPolicyView = {
  /** Global layer: whether Bypass may be selected. */
  bypassAvailable: boolean;
  bypassAvailableSource: "global" | "default";
  /**
   * Project layer selection when a projectId was requested; null when no project.
   */
  projectMode: PermissionModeLabel | null;
  projectModeSource: "project" | "default" | null;
  /** Resolved approval mode after two-tier gate. */
  effectiveMode: PermissionModeLabel;
};

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
  /**
   * Host authority (ADR-0003 Q2): whether Fake may appear in the adapter
   * select. Renderer must filter options from this flag — never hard-code
   * import.meta.env / NODE_ENV detection.
   */
  fakeAdapterSelectable: boolean;
};

/**
 * Process-startup conversation runtime status.
 * When degraded, conversation is off but Settings remains reachable.
 */
export type RuntimeStartupView = {
  conversationAvailable: boolean;
  selection: string | null;
  degradedCode: string | null;
  degradedMessage: string | null;
  degradedGuidance: string | null;
};

/**
 * Renderer-facing theme settings. Same settings store + precedence as runtime;
 * no secret fields.
 */
export type ThemeSettingsView = {
  theme: ResolvedSettingView;
};

/**
 * First-run onboarding dismiss flag (`ui.onboarding.dismissed`).
 * Global install fact; no env override; no secret fields.
 * Absent store row → dismissed false (show guidance once).
 */
export type OnboardingSettingsView = {
  dismissed: boolean;
  /** `project` | `global` | `default` — never env for this key. */
  source: SettingSource;
};

export type SettingsGatewayErrorCode =
  | "invalid_runtime_selection"
  | "invalid_theme_selection"
  | "invalid_permission_mode"
  | "permission_mode_global_only"
  | "permission_policy_host_missing"
  | "permission_bypass_not_available"
  | "settings_invalid_scope"
  | "settings_scope_shape"
  | "settings_unknown_key"
  | "settings_invalid_path"
  | "settings_invalid_value_kind"
  | "settings_invalid_value_json"
  | "settings_sqlite"
  | "settings_lock_poisoned"
  | "runtime_fake_not_allowed"
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
  /**
   * Legacy effective mode for workbench indicator (no project argument).
   * Prefer `getPermissionPolicy` for the settings two-tier surface.
   */
  getPermissionMode(): Promise<PermissionModeSettingsView>;
  /**
   * Legacy write used by workbench "恢复默认". Implementations should clear
   * project override toward default and keep Bypass capability unchanged
   * unless product policy says otherwise.
   */
  setPermissionMode(mode: PermissionModeLabel): Promise<PermissionModeSettingsView>;
  /** Two-tier policy for the security settings category. */
  getPermissionPolicy(projectId?: string | null): Promise<PermissionPolicyView>;
  /** Global layer: enable/disable Bypass as a selectable capability. */
  setBypassAvailable(available: boolean): Promise<PermissionPolicyView>;
  /**
   * Project layer: select mode for a project.
   * Host must reject `bypass` when global capability is off.
   */
  setProjectPermissionMode(
    projectId: string,
    mode: PermissionModeLabel,
  ): Promise<PermissionPolicyView>;
  getRuntimeSettings(projectId?: string | null): Promise<RuntimeSettingsView>;
  /** Optional: process startup ready/degraded status (Tauri host). */
  getRuntimeStartup?(): Promise<RuntimeStartupView>;
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
    key:
      | "runtime.adapter"
      | "runtime.grok.executable"
      | "runtime.grok.profile"
      | "ui.theme",
  ): Promise<RuntimeSettingsView>;
  getThemeSettings(projectId?: string | null): Promise<ThemeSettingsView>;
  /** Global-only write. Product UI does not expose project-scoped theme. */
  setUiTheme(theme: UiThemeLabel): Promise<ThemeSettingsView>;
  /** Global install fact: whether first-run onboarding was dismissed/skipped. */
  getOnboardingSettings(): Promise<OnboardingSettingsView>;
  /** Persist dismiss/skip so guidance does not re-nag after restart. */
  setOnboardingDismissed(dismissed: boolean): Promise<OnboardingSettingsView>;
}

/** Apply resolved theme to the document before paint-sensitive UI. */
export function applyDocumentTheme(theme: UiThemeLabel): void {
  if (typeof document === "undefined") return;
  document.documentElement.setAttribute("data-theme", theme);
  document.documentElement.setAttribute("data-theme-ready", "");
}

export function parseUiThemeLabel(value: string | null | undefined): UiThemeLabel {
  return value === "light" ? "light" : "dark";
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

/**
 * Two-tier resolution: project cannot break out of a global Bypass deny.
 * Fail-closed: requested bypass with capability off → `default`.
 */
export function resolveEffectivePermissionMode(
  bypassAvailable: boolean,
  mode: PermissionModeLabel,
): PermissionModeLabel {
  if (mode === "bypass" && !bypassAvailable) {
    return "default";
  }
  return mode;
}
