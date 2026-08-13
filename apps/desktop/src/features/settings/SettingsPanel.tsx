import { useCallback, useEffect, useState } from "react";
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
  BYPASS_AVAILABLE_HELP,
  BYPASS_AVAILABLE_LABEL,
  BYPASS_PROJECT_LOCKED_NOTE,
  PERMISSION_DENY_RECEIPT_NOTE,
  PERMISSION_MODE_OPTIONS,
  PERMISSION_TWO_AXIS_NOTE,
  PROJECT_MODE_HELP,
  PROJECT_MODE_NO_PROJECT_NOTE,
  permissionModeOption,
} from "./permissionCopy";
import { RuntimeSlotsSection } from "./RuntimeSlotsSection";
import { LibrarianMonitorSection } from "./LibrarianMonitorSection";
import {
  applyDocumentTheme,
  isEnvOverride,
  parseUiThemeLabel,
  sourceLabel,
  type PermissionModeLabel,
  type PermissionPolicyView,
  type RuntimeAdapterLabel,
  type RuntimeSettingsView,
  type SettingsGateway,
  type SettingsScope,
  type ThemeSettingsView,
  type UiThemeLabel,
} from "./settingsTypes";
import type { RuntimeSlotsGateway } from "./runtimeSlotsTypes";
import type { LibrarianAuditGateway } from "./librarianAuditTypes";
import "../../styles/settings.css";

export type SettingsPanelProps = {
  gateway: SettingsGateway;
  /** Active project id when a project is selected; enables project scope. */
  projectId: string | null;
  /** Opens the project picker without leaving Settings, for sections gated on a project. */
  onOpenProject?: () => void;
  onClose: () => void;
  /** Called after a successful theme write so the shell can re-apply. */
  onThemeChanged?: (theme: UiThemeLabel) => void;
  /** Keeps the workbench's persistent mode indicator synchronized. */
  onPermissionModeChanged?: (mode: PermissionModeLabel) => void;
  /**
   * Optional permission-rules gateway. Defaults to Tauri when the IPC bridge
   * is present; otherwise an in-memory mock (unit tests / non-desktop).
   */
  permissionRulesGateway?: PermissionRulesGateway;
  /**
   * Optional runtime-slots gateway. Defaults to Tauri when the IPC bridge
   * is present; otherwise an in-memory mock (unit tests / non-desktop).
   */
  runtimeSlotsGateway?: RuntimeSlotsGateway;
  /** Injectable project-health gateway; production defaults inside the section. */
  librarianAuditGateway?: LibrarianAuditGateway;
};

/** Settings category ids — IDE-style left nav (VS Code / Cursor / Zed). */
export type SettingsCategoryId =
  | "security"
  | "agents"
  | "runtime"
  | "appearance"
  | "diagnostics";

export const SETTINGS_CATEGORIES: {
  id: SettingsCategoryId;
  label: string;
  description: string;
}[] = [
  {
    id: "security",
    label: "安全与权限",
    description: "批准策略、两级 Bypass、权限规则",
  },
  {
    id: "agents",
    label: "代理与槽位",
    description: "运行时槽位、角色与模型",
  },
  {
    id: "runtime",
    label: "运行时",
    description: "适配器与 Grok 路径（高级）",
  },
  {
    id: "appearance",
    label: "外观",
    description: "主题",
  },
  {
    id: "diagnostics",
    label: "维护与诊断",
    description: "项目体检",
  },
];

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
 * Settings surface: IDE-style category nav + two-tier permission policy.
 * Env overrides are visible and read-only (ADR-0003).
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
  onOpenProject,
  onClose,
  onThemeChanged,
  onPermissionModeChanged,
  permissionRulesGateway: permissionRulesGatewayProp,
  runtimeSlotsGateway: runtimeSlotsGatewayProp,
  librarianAuditGateway,
}: SettingsPanelProps) {
  const [category, setCategory] = useState<SettingsCategoryId>("security");
  const [scope, setScope] = useState<SettingsScope>("global");
  const [settings, setSettings] = useState<RuntimeSettingsView | null>(null);
  const [themeSettings, setThemeSettings] = useState<ThemeSettingsView | null>(
    null,
  );
  const [policy, setPolicy] = useState<PermissionPolicyView | null>(null);
  const [adapterDraft, setAdapterDraft] = useState<RuntimeAdapterLabel>("grok-acp");
  const [themeDraft, setThemeDraft] = useState<UiThemeLabel>("dark");
  const [bypassAvailableDraft, setBypassAvailableDraft] = useState(true);
  const [projectModeDraft, setProjectModeDraft] =
    useState<PermissionModeLabel>("default");
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

  const applyPolicyView = useCallback((view: PermissionPolicyView) => {
    setPolicy(view);
    setBypassAvailableDraft(view.bypassAvailable);
    if (view.projectMode) {
      setProjectModeDraft(view.projectMode);
    } else {
      // No project: seed draft from effective so the select is not empty when opened later.
      setProjectModeDraft(view.effectiveMode);
    }
  }, []);

  const reload = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [view, themeView, policyView] = await Promise.all([
        gateway.getRuntimeSettings(effectiveProjectId),
        // Theme is global-only in the product UI; resolve without project so
        // project-scoped rows (if any) do not affect the editor value.
        gateway.getThemeSettings(null),
        gateway.getPermissionPolicy(projectId),
      ]);
      applyView(view);
      applyThemeView(themeView);
      applyPolicyView(policyView);
    } catch (cause) {
      const message =
        cause instanceof Error ? cause.message : "加载设置失败。";
      setError(message);
    } finally {
      setLoading(false);
    }
  }, [
    applyPolicyView,
    applyThemeView,
    applyView,
    effectiveProjectId,
    gateway,
    projectId,
  ]);

  useEffect(() => {
    void reload();
  }, [reload]);

  // Project scope unavailable without an active project.
  useEffect(() => {
    if (scope === "project" && !projectId) {
      setScope("global");
    }
  }, [projectId, scope]);

  async function handleSave() {
    if (!settings) return;
    setSaving(true);
    setError(null);
    setNotice(null);
    try {
      let view = settings;
      let policyView = policy;

      if (category === "security") {
        policyView = await gateway.setBypassAvailable(bypassAvailableDraft);
        applyPolicyView(policyView);
        if (projectId) {
          // Clamp draft if capability just turned off while Bypass was selected.
          const modeToWrite =
            projectModeDraft === "bypass" && !policyView.bypassAvailable
              ? "default"
              : projectModeDraft;
          if (modeToWrite !== projectModeDraft) {
            setProjectModeDraft(modeToWrite);
          }
          policyView = await gateway.setProjectPermissionMode(
            projectId,
            modeToWrite,
          );
          applyPolicyView(policyView);
        }
        onPermissionModeChanged?.(policyView.effectiveMode);
      }

      if (category === "appearance") {
        if (themeSettings && !isEnvOverride(themeSettings.theme)) {
          const themeView = await gateway.setUiTheme(themeDraft);
          applyThemeView(themeView);
          const applied = parseUiThemeLabel(themeView.theme.value);
          applyDocumentTheme(applied);
          onThemeChanged?.(applied);
        }
      }

      if (category === "runtime") {
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
        applyView(view);
      }

      const noticeByCategory: Record<SettingsCategoryId, string> = {
        security: "已保存。权限策略立即生效。",
        appearance: "已保存。主题立即生效。",
        runtime:
          "已保存。运行时适配器与 Grok 路径在重启 Pillow 后生效。",
        agents: "已保存。",
        diagnostics: "已保存。",
      };
      setNotice(noticeByCategory[category]);
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

  const bypassOptionDisabled = !bypassAvailableDraft;
  const projectModeSelectDisabled = saving || !projectId;
  const activeCategoryMeta = SETTINGS_CATEGORIES.find((c) => c.id === category);

  const showSave =
    category === "security"
    || category === "runtime"
    || category === "appearance";

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
            <p>按功能分区。非密钥运行时配置；Grok 登录由 Grok CLI 管理。</p>
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

        <div className="settings-layout">
          <nav
            aria-label="设置分类"
            className="settings-nav"
            data-testid="settings-nav"
          >
            {SETTINGS_CATEGORIES.map((item) => (
              <button
                key={item.id}
                aria-current={category === item.id ? "page" : undefined}
                className={
                  category === item.id
                    ? "settings-nav__item settings-nav__item--active"
                    : "settings-nav__item"
                }
                data-testid={`settings-nav-${item.id}`}
                onClick={() => {
                  setCategory(item.id);
                  setNotice(null);
                  setError(null);
                }}
                type="button"
              >
                <span className="settings-nav__label">{item.label}</span>
                <span className="settings-nav__desc">{item.description}</span>
              </button>
            ))}
          </nav>

          <div className="settings-content" data-testid="settings-content">
            {activeCategoryMeta ? (
              <header className="settings-content__head">
                <h3 data-testid="settings-category-title">
                  {activeCategoryMeta.label}
                </h3>
                <p className="settings-content__lede">
                  {activeCategoryMeta.description}
                </p>
              </header>
            ) : null}

            {loading ? (
              <p className="settings-status">正在加载设置…</p>
            ) : (
              /* Not a <form>: child sections (permission rules, slots) own
               * their own forms; nesting would be invalid HTML. */
              <div className="settings-form">
                {category === "security" ? (
                  <SecurityCategory
                    bypassAvailableDraft={bypassAvailableDraft}
                    bypassOptionDisabled={bypassOptionDisabled}
                    onBypassAvailableChange={setBypassAvailableDraft}
                    onProjectModeChange={setProjectModeDraft}
                    permissionRulesGateway={permissionRulesGateway}
                    policy={policy}
                    projectId={projectId}
                    projectModeDraft={projectModeDraft}
                    projectModeSelectDisabled={projectModeSelectDisabled}
                    saving={saving}
                  />
                ) : null}

                {category === "agents" ? (
                  <RuntimeSlotsSection
                    gateway={runtimeSlotsGatewayProp}
                    onOpenProject={onOpenProject}
                    projectId={projectId}
                  />
                ) : null}

                {category === "runtime" ? (
                  <RuntimeCategory
                    adapterDraft={adapterDraft}
                    adapterLocked={adapterLocked}
                    adapterSelectOptions={adapterSelectOptions}
                    executableDraft={executableDraft}
                    executableLocked={executableLocked}
                    onAdapterChange={setAdapterDraft}
                    onExecutableChange={setExecutableDraft}
                    onProfileChange={setProfileDraft}
                    onScopeChange={setScope}
                    profileDraft={profileDraft}
                    profileLocked={profileLocked}
                    projectId={projectId}
                    saving={saving}
                    scope={scope}
                    settings={settings}
                  />
                ) : null}

                {category === "appearance" ? (
                  <AppearanceCategory
                    onThemeChange={setThemeDraft}
                    saving={saving}
                    themeDraft={themeDraft}
                    themeLocked={themeLocked}
                    themeSettings={themeSettings}
                  />
                ) : null}

                {category === "diagnostics" ? (
                  <LibrarianMonitorSection
                    auditGateway={librarianAuditGateway}
                    projectId={projectId}
                    runtimeSlotsGateway={runtimeSlotsGatewayProp}
                  />
                ) : null}

                {error ? (
                  <p
                    className="settings-error"
                    data-testid="settings-error"
                    role="alert"
                  >
                    {error}
                  </p>
                ) : null}
                {notice ? (
                  <p className="settings-notice" data-testid="settings-notice">
                    {notice}
                  </p>
                ) : null}

                {/*
                  No per-section close button. The dialog has one close affordance
                  in its header; repeating it at the bottom of every section makes
                  it read as "close this section", which is not a thing.
                */}
                {showSave ? (
                  <div className="settings-actions modal-actions">
                    <button
                      data-testid="settings-save"
                      disabled={
                        saving
                        || loading
                        || (
                          category === "runtime"
                          && adapterLocked
                          && executableLocked
                          && profileLocked
                        )
                        || (
                          category === "appearance"
                          && themeLocked
                        )
                      }
                      onClick={() => void handleSave()}
                      type="button"
                    >
                      {saving ? "保存中…" : "保存"}
                    </button>
                  </div>
                ) : null}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function SecurityCategory({
  bypassAvailableDraft,
  bypassOptionDisabled,
  onBypassAvailableChange,
  onProjectModeChange,
  permissionRulesGateway,
  policy,
  projectId,
  projectModeDraft,
  projectModeSelectDisabled,
  saving,
}: {
  bypassAvailableDraft: boolean;
  bypassOptionDisabled: boolean;
  onBypassAvailableChange: (value: boolean) => void;
  onProjectModeChange: (mode: PermissionModeLabel) => void;
  permissionRulesGateway: PermissionRulesGateway;
  policy: PermissionPolicyView | null;
  projectId: string | null;
  projectModeDraft: PermissionModeLabel;
  projectModeSelectDisabled: boolean;
  saving: boolean;
}) {
  const modeDetail = permissionModeOption(projectModeDraft).detail;
  const showDenyReceipt =
    projectModeDraft === "accept_edits" || projectModeDraft === "bypass";

  return (
    <div className="settings-category" data-testid="settings-category-security">
      <p
        className="settings-two-axis-note"
        data-testid="settings-permission-two-axis-note"
        role="note"
      >
        {PERMISSION_TWO_AXIS_NOTE}
      </p>

      <section
        className="settings-section"
        data-testid="settings-permission-global"
      >
        <h4 className="settings-section__title">全局层 · Bypass 能力</h4>
        <p className="settings-section__help">{BYPASS_AVAILABLE_HELP}</p>
        <label className="settings-field settings-field--row">
          <span className="settings-field__label">
            {BYPASS_AVAILABLE_LABEL}
            {policy ? (
              <SourceBadge source={policy.bypassAvailableSource} />
            ) : null}
          </span>
          <input
            aria-label={BYPASS_AVAILABLE_LABEL}
            checked={bypassAvailableDraft}
            className="settings-field__checkbox"
            data-testid="settings-bypass-available"
            disabled={saving}
            onChange={(event) => onBypassAvailableChange(event.target.checked)}
            type="checkbox"
          />
        </label>
      </section>

      <section
        className="settings-section"
        data-testid="settings-permission-project"
      >
        <h4 className="settings-section__title">项目层 · 批准模式</h4>
        <p className="settings-section__help">{PROJECT_MODE_HELP}</p>
        <label className="settings-field">
          <span className="settings-field__label">
            权限模式
            {policy?.projectModeSource ? (
              <SourceBadge source={policy.projectModeSource} />
            ) : null}
            {policy ? (
              <span
                className="settings-source"
                data-testid="settings-permission-effective"
                title="两级求值后的有效模式"
              >
                有效：{permissionModeOption(policy.effectiveMode).label}
              </span>
            ) : null}
          </span>
          <select
            aria-label="权限模式"
            className="settings-field__control"
            data-testid="settings-permission-mode"
            disabled={projectModeSelectDisabled}
            onChange={(event) =>
              onProjectModeChange(event.target.value as PermissionModeLabel)
            }
            title={
              !projectId
                ? "打开项目后可编辑项目权限模式"
                : undefined
            }
            value={projectModeDraft}
          >
            {PERMISSION_MODE_OPTIONS.map((option) => (
              <option
                disabled={option.value === "bypass" && bypassOptionDisabled}
                key={option.value}
                value={option.value}
              >
                {option.label}
              </option>
            ))}
          </select>
          {!projectId ? (
            <p
              className="settings-env-note"
              data-testid="settings-permission-mode-no-project"
              role="status"
            >
              {PROJECT_MODE_NO_PROJECT_NOTE}
            </p>
          ) : bypassOptionDisabled && projectModeDraft === "bypass" ? (
            <p
              className="settings-env-note"
              data-testid="settings-bypass-project-locked"
              role="status"
            >
              {BYPASS_PROJECT_LOCKED_NOTE}
            </p>
          ) : bypassOptionDisabled ? (
            <p
              className="settings-env-note"
              data-testid="settings-bypass-project-locked"
              role="status"
            >
              {BYPASS_PROJECT_LOCKED_NOTE}
            </p>
          ) : (
            <p
              className="settings-field__detail"
              data-testid="settings-permission-mode-detail"
              role="note"
            >
              {modeDetail}
            </p>
          )}
          {projectId && showDenyReceipt ? (
            <p className="settings-permission-warning" role="note">
              {PERMISSION_DENY_RECEIPT_NOTE}
            </p>
          ) : null}
        </label>
      </section>

      <PermissionRulesSection
        gateway={permissionRulesGateway}
        projectId={projectId}
      />
    </div>
  );
}

function RuntimeCategory({
  adapterDraft,
  adapterLocked,
  adapterSelectOptions,
  executableDraft,
  executableLocked,
  onAdapterChange,
  onExecutableChange,
  onProfileChange,
  onScopeChange,
  profileDraft,
  profileLocked,
  projectId,
  saving,
  scope,
  settings,
}: {
  adapterDraft: RuntimeAdapterLabel;
  adapterLocked: boolean;
  adapterSelectOptions: { value: RuntimeAdapterLabel; label: string }[];
  executableDraft: string;
  executableLocked: boolean;
  onAdapterChange: (value: RuntimeAdapterLabel) => void;
  onExecutableChange: (value: string) => void;
  onProfileChange: (value: string) => void;
  onScopeChange: (scope: SettingsScope) => void;
  profileDraft: string;
  profileLocked: boolean;
  projectId: string | null;
  saving: boolean;
  scope: SettingsScope;
  settings: RuntimeSettingsView | null;
}) {
  return (
    <div className="settings-category" data-testid="settings-category-runtime">
      <p className="settings-advanced-badge" data-testid="settings-runtime-advanced">
        高级 · 安装级运行时覆盖
      </p>
      <p className="settings-section__help">
        适配器与 Grok 可执行文件影响宿主如何启动治理对话。profile 路径
        <strong>只影响登录型槽位</strong>
        ；API key 槽位使用各自私有 home，不受此字段支配。
      </p>

      <div className="settings-scope" role="group" aria-label="设置作用域">
        <button
          aria-pressed={scope === "global"}
          className={
            scope === "global"
              ? "settings-scope__btn settings-scope__btn--active"
              : "settings-scope__btn"
          }
          onClick={() => onScopeChange("global")}
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
          onClick={() => onScopeChange("project")}
          title={projectId ? "当前项目设置" : "打开项目后可编辑项目设置"}
          type="button"
        >
          当前项目
        </button>
      </div>

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
            onAdapterChange(e.target.value as RuntimeAdapterLabel)
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
          onChange={(e) => onExecutableChange(e.target.value)}
          placeholder="例如 C:\tools\grok.exe（留空则使用 PATH）"
          spellCheck={false}
          type="text"
          value={executableDraft}
        />
        <p className="settings-field__detail" role="note">
          本机 Grok CLI 安装位置。治理 Grok 槽共用此二进制（非每槽位路径）。
        </p>
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
          <span className="settings-advanced-inline">高级</span>
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
          onChange={(e) => onProfileChange(e.target.value)}
          placeholder="进程主目录（仅路径；不会打开 auth 文件）"
          spellCheck={false}
          type="text"
          value={profileDraft}
        />
        <p
          className="settings-field__detail"
          data-testid="settings-profile-scope-note"
          role="note"
        >
          仅影响<strong>登录型槽位</strong>的配置根覆盖。API key
          槽位使用 Pillow 按槽生成的私有 home，不受此字段支配。
        </p>
        {profileLocked ? (
          <EnvOverrideNote
            envVar={settings?.grokProfile.envVar ?? "PILLOW_GROK_PROFILE"}
            testId="settings-profile-env-override"
          />
        ) : null}
      </label>
    </div>
  );
}

function AppearanceCategory({
  onThemeChange,
  saving,
  themeDraft,
  themeLocked,
  themeSettings,
}: {
  onThemeChange: (theme: UiThemeLabel) => void;
  saving: boolean;
  themeDraft: UiThemeLabel;
  themeLocked: boolean;
  themeSettings: ThemeSettingsView | null;
}) {
  return (
    <div className="settings-category" data-testid="settings-category-appearance">
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
          disabled={themeLocked || saving}
          onChange={(e) => onThemeChange(e.target.value as UiThemeLabel)}
          value={themeDraft}
        >
          {THEME_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>
        <p className="settings-field__detail" role="note">
          主题仅支持全局设置。
        </p>
        {themeLocked ? (
          <EnvOverrideNote
            envVar={themeSettings?.theme.envVar ?? "PILLOW_THEME"}
            testId="settings-theme-env-override"
          />
        ) : null}
      </label>
    </div>
  );
}

function SourceBadge({
  source,
  envVar,
}: {
  source: RuntimeSettingsView["adapter"]["source"] | "global" | "default" | "project";
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
      {env ? "已被环境变量覆盖" : sourceLabel(source as RuntimeSettingsView["adapter"]["source"])}
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
