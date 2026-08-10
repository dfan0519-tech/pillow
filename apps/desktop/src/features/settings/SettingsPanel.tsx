import { useCallback, useEffect, useState, type FormEvent } from "react";
import {
  MockPermissionRulesGateway,
  TauriPermissionRulesGateway,
  type PermissionRulesGateway,
} from "../../lib/permissionRulesGateway";
import { PermissionRulesSection } from "../permissions/PermissionRulesSection";
import {
  GROK_CLI_LOGIN_SETTINGS_HELP,
} from "./loginGuidance";
import {
  applyDocumentTheme,
  isEnvOverride,
  parseUiThemeLabel,
  sourceLabel,
  type RuntimeAdapterLabel,
  type RuntimeSettingsView,
  type SettingsGateway,
  type SettingsScope,
  type ThemeSettingsView,
  type UiThemeLabel,
} from "./settingsTypes";
import "../../styles/settings.css";

export type SettingsPanelProps = {
  gateway: SettingsGateway;
  /** Active project id when a project is selected; enables project scope. */
  projectId: string | null;
  onClose: () => void;
  /** Called after a successful theme write so the shell can re-apply. */
  onThemeChanged?: (theme: UiThemeLabel) => void;
  /**
   * Optional permission-rules gateway. Defaults to Tauri when the IPC bridge
   * is present; otherwise an in-memory mock (unit tests / non-desktop).
   */
  permissionRulesGateway?: PermissionRulesGateway;
};

const ADAPTER_OPTIONS_BASE: { value: RuntimeAdapterLabel; label: string }[] = [
  { value: "grok-acp", label: "Grok ACP（默认）" },
];

const ADAPTER_OPTION_FAKE: { value: RuntimeAdapterLabel; label: string } = {
  value: "fake",
  label: "Fake（开发/测试）",
};

/** Build adapter options from host flag (ADR-0003 Q2). Never hard-code env. */
export function adapterOptionsForBuild(
  fakeAdapterSelectable: boolean,
): { value: RuntimeAdapterLabel; label: string }[] {
  if (fakeAdapterSelectable) {
    return [...ADAPTER_OPTIONS_BASE, ADAPTER_OPTION_FAKE];
  }
  return [...ADAPTER_OPTIONS_BASE];
}

const THEME_OPTIONS: { value: UiThemeLabel; label: string }[] = [
  { value: "dark", label: "深色（默认）" },
  { value: "light", label: "浅色" },
];

/**
 * S0 settings surface: runtime adapter + Grok paths + global UI theme.
 * Env overrides are visible and read-only (ADR-0003).
 * Content form — high contrast, small radius, no per-row glass.
 */
function defaultPermissionRulesGateway(): PermissionRulesGateway {
  if (
    typeof window !== "undefined"
    && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  ) {
    return new TauriPermissionRulesGateway();
  }
  return new MockPermissionRulesGateway();
}

export function SettingsPanel({
  gateway,
  projectId,
  onClose,
  onThemeChanged,
  permissionRulesGateway: permissionRulesGatewayProp,
}: SettingsPanelProps) {
  const [scope, setScope] = useState<SettingsScope>("global");
  const [settings, setSettings] = useState<RuntimeSettingsView | null>(null);
  const [themeSettings, setThemeSettings] = useState<ThemeSettingsView | null>(
    null,
  );
  const [adapterDraft, setAdapterDraft] = useState<RuntimeAdapterLabel>("grok-acp");
  const [themeDraft, setThemeDraft] = useState<UiThemeLabel>("dark");
  const [executableDraft, setExecutableDraft] = useState("");
  const [profileDraft, setProfileDraft] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [permissionRulesGateway] = useState<PermissionRulesGateway>(
    () => permissionRulesGatewayProp ?? defaultPermissionRulesGateway(),
  );

  const effectiveProjectId = scope === "project" ? projectId : null;

  const applyView = useCallback((view: RuntimeSettingsView) => {
    setSettings(view);
    const adapter = view.adapter.value === "fake" ? "fake" : "grok-acp";
    setAdapterDraft(adapter);
    setExecutableDraft(view.grokExecutable.value ?? "");
    setProfileDraft(view.grokProfile.value ?? "");
  }, []);

  const applyThemeView = useCallback((view: ThemeSettingsView) => {
    setThemeSettings(view);
    setThemeDraft(parseUiThemeLabel(view.theme.value));
  }, []);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [view, themeView] = await Promise.all([
        gateway.getRuntimeSettings(effectiveProjectId),
        // Theme is global-only in the product UI; resolve without project so
        // project-scoped rows (if any) do not affect the editor value.
        gateway.getThemeSettings(null),
      ]);
      applyView(view);
      applyThemeView(themeView);
    } catch (cause) {
      const message =
        cause instanceof Error ? cause.message : "加载设置失败。";
      setError(message);
    } finally {
      setLoading(false);
    }
  }, [applyThemeView, applyView, effectiveProjectId, gateway]);

  useEffect(() => {
    void reload();
  }, [reload]);

  // Project scope unavailable without an active project.
  useEffect(() => {
    if (scope === "project" && !projectId) {
      setScope("global");
    }
  }, [projectId, scope]);

  async function handleSave(event: FormEvent) {
    event.preventDefault();
    if (!settings) return;
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      let view = settings;
      if (!isEnvOverride(settings.adapter)) {
        view = await gateway.setRuntimeAdapter(
          scope,
          effectiveProjectId,
          adapterDraft,
        );
      }
      if (!isEnvOverride(settings.grokExecutable)) {
        const trimmed = executableDraft.trim();
        if (trimmed.length > 0) {
          view = await gateway.setGrokExecutable(
            scope,
            effectiveProjectId,
            trimmed,
          );
        }
      }
      if (!isEnvOverride(settings.grokProfile)) {
        const trimmed = profileDraft.trim();
        if (trimmed.length > 0) {
          view = await gateway.setGrokProfile(
            scope,
            effectiveProjectId,
            trimmed,
          );
        }
      }
      // Theme is always global scope in the UI (never project).
      if (themeSettings && !isEnvOverride(themeSettings.theme)) {
        const themeView = await gateway.setUiTheme(themeDraft);
        applyThemeView(themeView);
        const applied = parseUiThemeLabel(themeView.theme.value);
        applyDocumentTheme(applied);
        onThemeChanged?.(applied);
      }
      applyView(view);
      setNotice("已保存。主题立即生效；运行时适配器与 Grok 路径在重启 Pillow 后生效。");
    } catch (cause) {
      const message =
        cause instanceof Error ? cause.message : "保存设置失败。";
      setError(message);
    } finally {
      setSaving(false);
    }
  }

  const adapterLocked = settings ? isEnvOverride(settings.adapter) : false;
  const executableLocked = settings
    ? isEnvOverride(settings.grokExecutable)
    : false;
  const profileLocked = settings ? isEnvOverride(settings.grokProfile) : false;
  const themeLocked = themeSettings
    ? isEnvOverride(themeSettings.theme)
    : false;

  const adapterOptions = adapterOptionsForBuild(
    settings?.fakeAdapterSelectable ?? false,
  );
  // If stored value is fake but host no longer offers it, keep showing the
  // current value so the user can change it (fail-closed, no silent rewrite).
  const adapterSelectOptions =
    adapterDraft === "fake"
    && !adapterOptions.some((opt) => opt.value === "fake")
      ? [...adapterOptions, ADAPTER_OPTION_FAKE]
      : adapterOptions;

  return (
    <div
      aria-labelledby="settings-panel-title"
      aria-modal="true"
      className="settings-backdrop"
      data-testid="settings-panel"
      role="dialog"
    >
      <div className="settings-panel modal-card">
        <header className="modal-card__head settings-panel__head">
          <div>
            <h2 id="settings-panel-title">设置</h2>
            <p>非密钥运行时配置。Grok 登录由 Grok CLI 管理，Pillow 不存储该登录。</p>
          </div>
          <button
            aria-label="关闭设置"
            className="settings-panel__close"
            onClick={onClose}
            type="button"
          >
            ×
          </button>
        </header>

        <p
          className="settings-login-help"
          data-testid="settings-login-guidance"
          role="note"
        >
          {GROK_CLI_LOGIN_SETTINGS_HELP}
        </p>

        <div className="settings-scope" role="group" aria-label="设置作用域">
          <button
            aria-pressed={scope === "global"}
            className={
              scope === "global"
                ? "settings-scope__btn settings-scope__btn--active"
                : "settings-scope__btn"
            }
            onClick={() => setScope("global")}
            type="button"
          >
            全局
          </button>
          <button
            aria-pressed={scope === "project"}
            className={
              scope === "project"
                ? "settings-scope__btn settings-scope__btn--active"
                : "settings-scope__btn"
            }
            disabled={!projectId}
            onClick={() => setScope("project")}
            title={projectId ? "当前项目设置" : "打开项目后可编辑项目设置"}
            type="button"
          >
            当前项目
          </button>
        </div>

        {loading ? (
          <p className="settings-status">正在加载设置…</p>
        ) : (
          <form className="settings-form" onSubmit={(e) => void handleSave(e)}>
            <label className="settings-field">
              <span className="settings-field__label">
                外观主题
                {themeSettings ? (
                  <SourceBadge
                    envVar={themeSettings.theme.envVar}
                    source={themeSettings.theme.source}
                  />
                ) : null}
              </span>
              <select
                aria-label="外观主题"
                className="settings-field__control"
                data-testid="settings-theme"
                disabled={themeLocked || saving || scope === "project"}
                onChange={(e) =>
                  setThemeDraft(e.target.value as UiThemeLabel)
                }
                title={
                  scope === "project"
                    ? "主题仅支持全局设置"
                    : undefined
                }
                value={themeDraft}
              >
                {THEME_OPTIONS.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
              {scope === "project" && !themeLocked ? (
                <p className="settings-env-note" role="status">
                  主题仅支持全局设置；切换到「全局」作用域可编辑。
                </p>
              ) : null}
              {themeLocked ? (
                <EnvOverrideNote
                  envVar={themeSettings?.theme.envVar ?? "PILLOW_THEME"}
                  testId="settings-theme-env-override"
                />
              ) : null}
            </label>

            <label className="settings-field">
              <span className="settings-field__label">
                运行时适配器
                {settings ? (
                  <SourceBadge
                    envVar={settings.adapter.envVar}
                    source={settings.adapter.source}
                  />
                ) : null}
              </span>
              <select
                aria-label="运行时适配器"
                className="settings-field__control"
                data-testid="settings-adapter"
                disabled={adapterLocked || saving}
                onChange={(e) =>
                  setAdapterDraft(e.target.value as RuntimeAdapterLabel)
                }
                value={adapterDraft}
              >
                {adapterSelectOptions.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
              {adapterLocked ? (
                <EnvOverrideNote
                  envVar={settings?.adapter.envVar ?? "PILLOW_RUNTIME"}
                  testId="settings-adapter-env-override"
                />
              ) : null}
              {settings
              && settings.adapter.value === "fake"
              && !settings.fakeAdapterSelectable
              && !adapterLocked ? (
                <p
                  className="settings-env-note"
                  data-testid="settings-fake-not-selectable"
                  role="status"
                >
                  当前构建不提供 Fake 适配器（仅开发构建可选）。请改为 Grok
                  ACP 后保存并重启。已存值不会被静默改写。
                </p>
              ) : null}
            </label>

            <label className="settings-field">
              <span className="settings-field__label">
                Grok 可执行文件路径
                {settings ? (
                  <SourceBadge
                    envVar={settings.grokExecutable.envVar}
                    source={settings.grokExecutable.source}
                  />
                ) : null}
              </span>
              <input
                aria-label="Grok 可执行文件路径"
                autoComplete="off"
                className="settings-field__control"
                data-testid="settings-grok-executable"
                disabled={executableLocked || saving}
                onChange={(e) => setExecutableDraft(e.target.value)}
                placeholder="例如 C:\tools\grok.exe（留空则使用 PATH）"
                spellCheck={false}
                type="text"
                value={executableDraft}
              />
              {executableLocked ? (
                <EnvOverrideNote
                  envVar={
                    settings?.grokExecutable.envVar ?? "PILLOW_GROK_EXECUTABLE"
                  }
                  testId="settings-executable-env-override"
                />
              ) : null}
            </label>

            <label className="settings-field">
              <span className="settings-field__label">
                Grok profile 路径
                {settings ? (
                  <SourceBadge
                    envVar={settings.grokProfile.envVar}
                    source={settings.grokProfile.source}
                  />
                ) : null}
              </span>
              <input
                aria-label="Grok profile 路径"
                autoComplete="off"
                className="settings-field__control"
                data-testid="settings-grok-profile"
                disabled={profileLocked || saving}
                onChange={(e) => setProfileDraft(e.target.value)}
                placeholder="进程主目录（仅路径；不会打开 auth 文件）"
                spellCheck={false}
                type="text"
                value={profileDraft}
              />
              {profileLocked ? (
                <EnvOverrideNote
                  envVar={settings?.grokProfile.envVar ?? "PILLOW_GROK_PROFILE"}
                  testId="settings-profile-env-override"
                />
              ) : null}
            </label>

            {error ? (
              <p className="settings-error" data-testid="settings-error" role="alert">
                {error}
              </p>
            ) : null}
            {notice ? (
              <p className="settings-notice" data-testid="settings-notice">
                {notice}
              </p>
            ) : null}

            <div className="settings-actions modal-actions">
              <button onClick={onClose} type="button">
                关闭
              </button>
              <button
                data-testid="settings-save"
                disabled={
                  saving
                  || loading
                  || (
                    adapterLocked
                    && executableLocked
                    && profileLocked
                    && (themeLocked || scope === "project")
                  )
                }
                type="submit"
              >
                {saving ? "保存中…" : "保存"}
              </button>
            </div>
          </form>
        )}

        <PermissionRulesSection
          gateway={permissionRulesGateway}
          projectId={projectId}
        />
      </div>
    </div>
  );
}

function SourceBadge({
  source,
  envVar,
}: {
  source: RuntimeSettingsView["adapter"]["source"];
  envVar?: string | null;
}) {
  const env = source === "env";
  return (
    <span
      className={
        env
          ? "settings-source settings-source--env"
          : "settings-source"
      }
      data-source={source}
      title={env && envVar ? `由 ${envVar} 覆盖` : undefined}
    >
      {env ? "已被环境变量覆盖" : sourceLabel(source)}
    </span>
  );
}

function EnvOverrideNote({
  envVar,
  testId,
}: {
  envVar: string;
  testId: string;
}) {
  return (
    <p className="settings-env-note" data-testid={testId} role="status">
      当前进程值由环境变量 <code>{envVar}</code> 覆盖，此项只读。
      清除该环境变量后，将使用下方保存的设置。
    </p>
  );
}
