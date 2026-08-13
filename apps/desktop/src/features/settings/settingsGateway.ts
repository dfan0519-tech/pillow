import { invoke } from "@tauri-apps/api/core";
import {
  SettingsGatewayError,
  resolveEffectivePermissionMode,
  type OnboardingSettingsView,
  type PermissionModeLabel,
  type PermissionModeSettingsView,
  type PermissionPolicyView,
  type RuntimeAdapterLabel,
  type RuntimeSettingsView,
  type RuntimeStartupView,
  type SettingSource,
  type SettingsGateway,
  type SettingsScope,
  type ThemeSettingsView,
  type UiThemeLabel,
} from "./settingsTypes";

const INVALID_RESPONSE = "Pillow received an invalid settings response.";
const HOST_POLICY_MISSING =
  "宿主尚未提供两级权限接口（get_permission_policy / set_bypass_available / set_project_permission_mode）。";

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

function isPermissionMode(value: unknown): value is PermissionModeLabel {
  return value === "default" || value === "accept_edits" || value === "bypass";
}

function hasExactKeys(value: Record<string, unknown>, expected: readonly string[]): boolean {
  const actual = Object.keys(value).sort();
  const allowed = [...expected].sort();
  return actual.length === allowed.length
    && actual.every((key, index) => key === allowed[index]);
}

/** Closed-key validator paired with the Rust `PermissionModeSettingsDto`. */
export function parsePermissionModeSettings(value: unknown): PermissionModeSettingsView {
  assertNoSecretFields(value);
  if (
    !isRecord(value)
    || !hasExactKeys(value, ["mode", "source"])
    || !isPermissionMode(value.mode)
    || !(value.source === "global" || value.source === "default")
  ) {
    throw new SettingsGatewayError("invalid_settings_response", INVALID_RESPONSE);
  }
  return { mode: value.mode, source: value.source };
}

/**
 * Closed-key validator for the two-tier policy DTO.
 * Host must send exactly these keys — adding fields is a breaking change.
 */
export function parsePermissionPolicy(value: unknown): PermissionPolicyView {
  assertNoSecretFields(value);
  if (
    !isRecord(value)
    || !hasExactKeys(value, [
      "bypassAvailable",
      "bypassAvailableSource",
      "projectMode",
      "projectModeSource",
      "effectiveMode",
    ])
    || typeof value.bypassAvailable !== "boolean"
    || !(value.bypassAvailableSource === "global" || value.bypassAvailableSource === "default")
    || !(value.projectMode === null || isPermissionMode(value.projectMode))
    || !(
      value.projectModeSource === null
      || value.projectModeSource === "project"
      || value.projectModeSource === "default"
    )
    || !isPermissionMode(value.effectiveMode)
  ) {
    throw new SettingsGatewayError("invalid_settings_response", INVALID_RESPONSE);
  }
  // Fail-closed: never accept effective bypass when capability is off.
  const effectiveMode = resolveEffectivePermissionMode(
    value.bypassAvailable,
    value.effectiveMode,
  );
  return {
    bypassAvailable: value.bypassAvailable,
    bypassAvailableSource: value.bypassAvailableSource,
    projectMode: value.projectMode,
    projectModeSource: value.projectModeSource,
    effectiveMode,
  };
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
    // Host is the authority; default true only when the field is omitted so
    // older mock fixtures in unit tests keep working. Production Tauri always
    // sends an explicit boolean from cfg!(debug_assertions).
    fakeAdapterSelectable:
      typeof value.fakeAdapterSelectable === "boolean"
        ? value.fakeAdapterSelectable
        : true,
  };
}

function parseRuntimeStartup(value: unknown): RuntimeStartupView {
  assertNoSecretFields(value);
  if (!isRecord(value)) {
    throw new SettingsGatewayError("invalid_settings_response", INVALID_RESPONSE);
  }
  return {
    conversationAvailable: value.conversationAvailable === true,
    selection:
      typeof value.selection === "string" && value.selection.trim().length > 0
        ? value.selection
        : null,
    degradedCode:
      typeof value.degradedCode === "string" ? value.degradedCode : null,
    degradedMessage:
      typeof value.degradedMessage === "string" ? value.degradedMessage : null,
    degradedGuidance:
      typeof value.degradedGuidance === "string"
        ? value.degradedGuidance
        : null,
  };
}

function parseThemeSettings(value: unknown): ThemeSettingsView {
  assertNoSecretFields(value);
  if (!isRecord(value)) {
    throw new SettingsGatewayError("invalid_settings_response", INVALID_RESPONSE);
  }
  return {
    theme: parseResolvedSetting(value.theme, "theme"),
  };
}

function parseOnboardingSettings(value: unknown): OnboardingSettingsView {
  assertNoSecretFields(value);
  if (
    !isRecord(value)
    || typeof value.dismissed !== "boolean"
    || !isSettingSource(value.source)
  ) {
    throw new SettingsGatewayError("invalid_settings_response", INVALID_RESPONSE);
  }
  return {
    dismissed: value.dismissed,
    source: value.source,
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

async function invokeThemeSettings(
  command: string,
  args?: Record<string, unknown>,
): Promise<ThemeSettingsView> {
  try {
    const payload = args
      ? await invoke<unknown>(command, args)
      : await invoke<unknown>(command);
    return parseThemeSettings(payload);
  } catch (error) {
    throw normalizeSettingsError(error);
  }
}

async function invokePermissionModeSettings(
  command: string,
  args?: Record<string, unknown>,
): Promise<PermissionModeSettingsView> {
  try {
    const payload = args
      ? await invoke<unknown>(command, args)
      : await invoke<unknown>(command);
    return parsePermissionModeSettings(payload);
  } catch (error) {
    throw normalizeSettingsError(error);
  }
}

/** Tauri settings gateway — host owns storage; renderer never touches the DB. */
export class TauriSettingsGateway implements SettingsGateway {
  getPermissionMode(): Promise<PermissionModeSettingsView> {
    return invokePermissionModeSettings("get_permission_mode");
  }

  setPermissionMode(mode: PermissionModeLabel): Promise<PermissionModeSettingsView> {
    return invokePermissionModeSettings("set_permission_mode", { mode });
  }

  async getPermissionPolicy(
    projectId?: string | null,
  ): Promise<PermissionPolicyView> {
    try {
      const payload = await invoke<unknown>("get_permission_policy", {
        projectId: projectId ?? null,
      });
      return parsePermissionPolicy(payload);
    } catch (error) {
      const normalized = normalizeSettingsError(error);
      if (normalized.code === "invalid_settings_response") {
        throw normalized;
      }
      if (
        normalized.code === "settings_command_failed"
        || /not found|unknown command/i.test(normalized.message)
      ) {
        throw new SettingsGatewayError(
          "permission_policy_host_missing",
          HOST_POLICY_MISSING,
        );
      }
      throw normalized;
    }
  }

  async setBypassAvailable(available: boolean): Promise<PermissionPolicyView> {
    try {
      const payload = await invoke<unknown>("set_bypass_available", {
        available,
      });
      return parsePermissionPolicy(payload);
    } catch (error) {
      const normalized = normalizeSettingsError(error);
      throw new SettingsGatewayError(
        normalized.code === "settings_command_failed"
          ? "permission_policy_host_missing"
          : normalized.code,
        normalized.code === "settings_command_failed"
          ? HOST_POLICY_MISSING
          : normalized.message,
      );
    }
  }

  async setProjectPermissionMode(
    projectId: string,
    mode: PermissionModeLabel,
  ): Promise<PermissionPolicyView> {
    try {
      const payload = await invoke<unknown>("set_project_permission_mode", {
        projectId,
        mode,
      });
      return parsePermissionPolicy(payload);
    } catch (error) {
      const normalized = normalizeSettingsError(error);
      throw new SettingsGatewayError(
        normalized.code === "settings_command_failed"
          ? "permission_policy_host_missing"
          : normalized.code,
        normalized.code === "settings_command_failed"
          ? HOST_POLICY_MISSING
          : normalized.message,
      );
    }
  }

  getRuntimeSettings(projectId?: string | null): Promise<RuntimeSettingsView> {
    return invokeSettings("get_runtime_settings", {
      projectId: projectId ?? null,
    });
  }

  async getRuntimeStartup(): Promise<RuntimeStartupView> {
    try {
      const payload = await invoke<unknown>("get_runtime_startup");
      return parseRuntimeStartup(payload);
    } catch (error) {
      throw normalizeSettingsError(error);
    }
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
    key:
      | "runtime.adapter"
      | "runtime.grok.executable"
      | "runtime.grok.profile"
      | "ui.theme",
  ): Promise<RuntimeSettingsView> {
    return invokeSettings("clear_runtime_setting", {
      scope,
      projectId,
      key,
    });
  }

  getThemeSettings(projectId?: string | null): Promise<ThemeSettingsView> {
    return invokeThemeSettings("get_theme_settings", {
      projectId: projectId ?? null,
    });
  }

  setUiTheme(theme: UiThemeLabel): Promise<ThemeSettingsView> {
    return invokeThemeSettings("set_ui_theme", { theme });
  }

  async getOnboardingSettings(): Promise<OnboardingSettingsView> {
    try {
      const payload = await invoke<unknown>("get_onboarding_settings");
      return parseOnboardingSettings(payload);
    } catch (error) {
      throw normalizeSettingsError(error);
    }
  }

  async setOnboardingDismissed(
    dismissed: boolean,
  ): Promise<OnboardingSettingsView> {
    try {
      const payload = await invoke<unknown>("set_onboarding_dismissed", {
        dismissed,
      });
      return parseOnboardingSettings(payload);
    } catch (error) {
      throw normalizeSettingsError(error);
    }
  }
}

/** In-memory gateway for unit tests (no Tauri). */
export class MockSettingsGateway implements SettingsGateway {
  private state: RuntimeSettingsView;
  private themeState: ThemeSettingsView;
  private startup: RuntimeStartupView;
  private onboarding: OnboardingSettingsView;
  /** Global Bypass capability. */
  private bypassAvailable: boolean;
  private bypassAvailableSource: "global" | "default";
  /**
   * Installation preferred mode (used when no project context, and as the
   * seed for project default until a project row is written).
   */
  private preferredMode: PermissionModeLabel;
  /** Project-scoped mode selections. */
  private projectModes = new Map<string, PermissionModeLabel>();
  /** Last project id used by policy reads (for effective mode in setBypass). */
  private lastPolicyProjectId: string | null = null;

  constructor(
    initial?: Partial<RuntimeSettingsView>,
    themeInitial?: Partial<ThemeSettingsView>,
    startupInitial?: Partial<RuntimeStartupView>,
    onboardingInitial?: Partial<OnboardingSettingsView>,
    permissionModeInitial?: Partial<PermissionModeSettingsView>,
    policyInitial?: {
      bypassAvailable?: boolean;
      preferredMode?: PermissionModeLabel;
      projectModes?: Record<string, PermissionModeLabel>;
    },
  ) {
    this.state = {
      adapter: {
        value: "grok-acp",
        source: "default",
        envVar: null,
      },
      grokExecutable: { value: null, source: "default", envVar: null },
      grokProfile: { value: null, source: "default", envVar: null },
      projectId: null,
      // Unit tests run as "dev": Fake is selectable unless a fixture overrides.
      fakeAdapterSelectable: true,
      ...initial,
    };
    this.themeState = {
      theme: { value: "dark", source: "default", envVar: null },
      ...themeInitial,
    };
    this.startup = {
      conversationAvailable: true,
      selection: "grok-acp",
      degradedCode: null,
      degradedMessage: null,
      degradedGuidance: null,
      ...startupInitial,
    };
    // Fresh install default: not dismissed (guidance may show when empty shell).
    this.onboarding = {
      dismissed: false,
      source: "default",
      ...onboardingInitial,
    };
    const initialMode = permissionModeInitial?.mode ?? "default";
    this.preferredMode = policyInitial?.preferredMode ?? initialMode;
    // Capability defaults open so legacy fixtures that set mode=bypass still work.
    // Explicit policyInitial.bypassAvailable wins; otherwise open unless tests pin it.
    this.bypassAvailable = policyInitial?.bypassAvailable ?? true;
    this.bypassAvailableSource =
      policyInitial?.bypassAvailable !== undefined ? "global" : "default";
    if (policyInitial?.projectModes) {
      for (const [id, mode] of Object.entries(policyInitial.projectModes)) {
        this.projectModes.set(id, mode);
      }
    }
  }

  private buildPolicy(projectId: string | null | undefined): PermissionPolicyView {
    const pid =
      typeof projectId === "string" && projectId.trim().length > 0
        ? projectId
        : null;
    this.lastPolicyProjectId = pid;
    let projectMode: PermissionModeLabel | null = null;
    let projectModeSource: PermissionPolicyView["projectModeSource"] = null;
    if (pid) {
      if (this.projectModes.has(pid)) {
        projectMode = this.projectModes.get(pid)!;
        projectModeSource = "project";
      } else {
        projectMode = this.preferredMode;
        projectModeSource = "default";
      }
    }
    const modeForEffective = projectMode ?? this.preferredMode;
    const effectiveMode = resolveEffectivePermissionMode(
      this.bypassAvailable,
      modeForEffective,
    );
    return {
      bypassAvailable: this.bypassAvailable,
      bypassAvailableSource: this.bypassAvailableSource,
      projectMode,
      projectModeSource,
      effectiveMode,
    };
  }

  getPermissionModeSnapshot(): PermissionModeSettingsView {
    const effective = resolveEffectivePermissionMode(
      this.bypassAvailable,
      this.preferredMode,
    );
    return {
      mode: effective,
      source: this.preferredMode === "default" && this.bypassAvailableSource === "default"
        ? "default"
        : "global",
    };
  }

  getPermissionPolicySnapshot(
    projectId?: string | null,
  ): PermissionPolicyView {
    return this.buildPolicy(projectId);
  }

  async getPermissionMode(): Promise<PermissionModeSettingsView> {
    return this.getPermissionModeSnapshot();
  }

  async setPermissionMode(mode: PermissionModeLabel): Promise<PermissionModeSettingsView> {
    // Workbench "恢复默认" and legacy callers: set preferred mode.
    // Opening Bypass via legacy write also enables the capability (old one-axis UX).
    if (mode === "bypass") {
      this.bypassAvailable = true;
      this.bypassAvailableSource = "global";
    }
    this.preferredMode = mode;
    if (mode === "default") {
      // Reset project rows so effective falls back cleanly.
      this.projectModes.clear();
    }
    return this.getPermissionModeSnapshot();
  }

  async getPermissionPolicy(
    projectId?: string | null,
  ): Promise<PermissionPolicyView> {
    return this.buildPolicy(projectId);
  }

  async setBypassAvailable(available: boolean): Promise<PermissionPolicyView> {
    this.bypassAvailable = available;
    this.bypassAvailableSource = "global";
    return this.buildPolicy(this.lastPolicyProjectId);
  }

  async setProjectPermissionMode(
    projectId: string,
    mode: PermissionModeLabel,
  ): Promise<PermissionPolicyView> {
    if (mode === "bypass" && !this.bypassAvailable) {
      throw new SettingsGatewayError(
        "permission_bypass_not_available",
        "全局已关闭 Bypass，项目不能选择该模式。",
      );
    }
    this.projectModes.set(projectId, mode);
    // Keep preferred in sync so workbench indicator (no project arg) tracks last choice.
    this.preferredMode = mode;
    return this.buildPolicy(projectId);
  }

  /** Test seam: force degraded conversation runtime (not Fake). */
  setDegradedStartup(code: string, message: string, guidance: string): void {
    this.startup = {
      conversationAvailable: false,
      selection: null,
      degradedCode: code,
      degradedMessage: message,
      degradedGuidance: guidance,
    };
  }

  async getRuntimeStartup(): Promise<RuntimeStartupView> {
    return structuredClone(this.startup);
  }

  /** Test seam: force env override presentation. */
  setEnvOverride(
    field: "adapter" | "grokExecutable" | "grokProfile" | "theme",
    value: string,
    envVar: string,
  ): void {
    if (field === "theme") {
      this.themeState = {
        theme: { value, source: "env", envVar },
      };
      return;
    }
    this.state = {
      ...this.state,
      [field]: { value, source: "env", envVar },
    };
  }

  getSnapshot(): RuntimeSettingsView {
    return structuredClone(this.state);
  }

  getThemeSnapshot(): ThemeSettingsView {
    return structuredClone(this.themeState);
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
    key:
      | "runtime.adapter"
      | "runtime.grok.executable"
      | "runtime.grok.profile"
      | "ui.theme",
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
    if (key === "ui.theme" && this.themeState.theme.source !== "env") {
      this.themeState = {
        theme: { value: "dark", source: "default", envVar: null },
      };
    }
    return this.getRuntimeSettings(projectId);
  }

  async getThemeSettings(
    _projectId?: string | null,
  ): Promise<ThemeSettingsView> {
    return structuredClone(this.themeState);
  }

  async setUiTheme(theme: UiThemeLabel): Promise<ThemeSettingsView> {
    if (this.themeState.theme.source === "env") {
      // Env still wins for effective view (read-only in UI).
      return this.getThemeSettings();
    }
    this.themeState = {
      theme: { value: theme, source: "global", envVar: null },
    };
    return this.getThemeSettings();
  }

  /** Test seam: force dismiss state without going through setOnboardingDismissed. */
  setOnboardingDismissedSync(dismissed: boolean): void {
    this.onboarding = {
      dismissed,
      source: dismissed ? "global" : "default",
    };
  }

  getOnboardingSnapshot(): OnboardingSettingsView {
    return structuredClone(this.onboarding);
  }

  async getOnboardingSettings(): Promise<OnboardingSettingsView> {
    return structuredClone(this.onboarding);
  }

  async setOnboardingDismissed(
    dismissed: boolean,
  ): Promise<OnboardingSettingsView> {
    this.onboarding = {
      dismissed,
      source: "global",
    };
    return this.getOnboardingSettings();
  }
}
