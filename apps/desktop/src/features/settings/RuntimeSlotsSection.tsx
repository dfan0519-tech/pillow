/**
 * Project-scoped runtime slots: unified create (name → auth → role),
 * role reassignment, and API-key edit (keep-on-blank).
 * Secrets never enter localStorage / sessionStorage / URL; drafts clear on save.
 * Host errors are never console-logged (may carry secret-shaped detail).
 */

import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import {
  connectProviderMethods,
  formatOauthExpiryForDisplay,
  isOauthLoginFailureCode,
  mapOauthLoginErrorMessage,
  oauthFieldsRequiringUserInput,
} from "./oauthLoginUi";
import {
  CATALOG_API_DEFAULTS,
  DEFAULT_PROVIDER_PRESET_ID,
  SLOT_CONNECT_PROVIDER_GROUPS,
  SLOT_CONNECT_PROVIDERS,
  getProviderPreset,
  getSlotConnectProvider,
  inferConnectProviderId,
  type ProviderPresetId,
  type SlotConnectProvider,
} from "./providerPresets";
import { RuntimeSlotModelPicker } from "./RuntimeSlotModelPicker";
import {
  MockRuntimeSlotsGateway,
  TauriRuntimeSlotsGateway,
  classifyExternalCliStatus,
  formatLocalCliAuthLabel,
  mapGrokProbeFailureMessage,
  mapModelCatalogUnsupportedReason,
  mapRuntimeSlotErrorMessage,
  truncateModelIdForDisplay,
} from "./runtimeSlotsGateway";
import {
  CONTROLLER_REQUIRES_GOVERNED_AUTH_HINT,
  EXTERNAL_CLI_UNGOVERNED_BANNER,
  EXTERNAL_CLI_UNGOVERNED_BODY,
  EXTERNAL_CLI_UNGOVERNED_HEADING,
  GOVERNED_GROK_BADGE,
  GUIDANCE_LIBRARIAN_SUGGESTION,
  GUIDANCE_NO_CONTROLLER_BODY,
  GUIDANCE_NO_CONTROLLER_HEADING,
  GUIDANCE_NO_SLOTS_BODY,
  GUIDANCE_NO_SLOTS_HEADING,
  LOCAL_CLI_DEFAULT_STATUS_ARGS,
  LOCAL_CLI_DETECT_BUSY,
  LOCAL_CLI_DETECT_HINT,
  LOCAL_CLI_DETECT_NONE_FOUND,
  MODEL_CATALOG_UNSUPPORTED_ENDPOINT,
  OAUTH_AUTH_BADGE,
  OAUTH_BROWSER_OPEN_FAILED_MESSAGE,
  OAUTH_CATALOG_LOAD_BUSY,
  OAUTH_LOGIN_FORM_HINT,
  OAUTH_LOGIN_IN_PROGRESS,
  RUNTIME_SLOT_OAUTH_PROGRESS_EVENT,
  SECRET_CONFIGURED_LABEL,
  SECRET_NOT_CONFIGURED_LABEL,
  SLOT_AUTH_METHODS,
  SLOT_ROLE_OPTIONS,
  type LocalCliDetectionView,
  type OauthLoginSupplementInput,
  type OauthUserFieldId,
  type ProbeModelCatalogView,
  type ProviderLoginCatalogEntry,
  type RuntimeSlotModelSwitchView,
  type RuntimeSlotRole,
  type RuntimeSlotView,
  type RuntimeSlotsGateway,
  type SlotAuthKind,
  type SlotFormStep,
  RuntimeSlotsGatewayError,
} from "./runtimeSlotsTypes";
import "../../styles/runtime-slots.css";

export type RuntimeSlotsSectionProps = {
  projectId: string | null;
  /** Opens the project picker from the gated empty state, without leaving Settings. */
  onOpenProject?: () => void;
  gateway?: RuntimeSlotsGateway;
};

type FormMode = "closed" | "create" | "edit";

type ProbeResult = {
  slotId: string;
  ok: boolean;
  text: string;
};

function defaultSlotsGateway(): RuntimeSlotsGateway {
  if (
    typeof window !== "undefined"
    && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window)
  ) {
    return new TauriRuntimeSlotsGateway();
  }
  return new MockRuntimeSlotsGateway();
}

/** Split argv fields: whitespace-separated; empty → []. */
export function parseArgvField(value: string): string[] {
  return value
    .trim()
    .split(/\s+/u)
    .filter((part) => part.length > 0);
}

function roleLabel(slot: RuntimeSlotView): string {
  if (slot.role === "controller") return "总控";
  if (slot.role === "librarian") return "信息管理员";
  return slot.roleCustomName ?? "未分配";
}

function authAllowsController(authKind: SlotAuthKind): boolean {
  const method = SLOT_AUTH_METHODS.find((entry) => entry.id === authKind);
  return method?.allowsController ?? false;
}

function slotAllowsController(slot: RuntimeSlotView): boolean {
  return slot.kind === "governed_grok" && slot.governance === "full";
}

function mapSaveError(cause: unknown): string {
  if (cause instanceof RuntimeSlotsGatewayError) {
    if (isOauthLoginFailureCode(cause.code)) {
      return mapOauthLoginErrorMessage(cause.code);
    }
    return mapRuntimeSlotErrorMessage(cause.code);
  }
  if (cause instanceof Error && cause.message.trim()) {
    return cause.message;
  }
  return "保存槽位失败。";
}

function authorizeUrlFromError(cause: unknown): string | null {
  if (
    cause instanceof RuntimeSlotsGatewayError
    && typeof cause.authorizeUrl === "string"
    && cause.authorizeUrl.trim().length > 0
  ) {
    return cause.authorizeUrl.trim();
  }
  return null;
}

type OauthProgressPayload = {
  authorizeUrl?: unknown;
  browserOpened?: unknown;
  userCode?: unknown;
};

function parseOauthProgressPayload(
  payload: unknown,
): { authorizeUrl: string; browserOpened: boolean; userCode: string | null } | null {
  if (!payload || typeof payload !== "object") return null;
  const record = payload as OauthProgressPayload;
  if (typeof record.authorizeUrl !== "string" || !record.authorizeUrl.trim()) {
    return null;
  }
  return {
    authorizeUrl: record.authorizeUrl.trim(),
    browserOpened: record.browserOpened === true,
    userCode:
      typeof record.userCode === "string" && record.userCode.trim()
        ? record.userCode.trim()
        : null,
  };
}

function authModeBadge(slot: RuntimeSlotView): string {
  if (slot.kind !== "governed_grok") return "本机 CLI";
  if (slot.authMode === "login") return "Grok · 登录";
  if (slot.authMode === "oauth") return OAUTH_AUTH_BADGE;
  return "Grok · API key";
}

type OauthDraftFields = Record<OauthUserFieldId, string>;

function emptyOauthDraftFields(): OauthDraftFields {
  return {
    clientId: "",
    issuer: "",
    authorizeEndpoint: "",
    deviceAuthorizationEndpoint: "",
    tokenEndpoint: "",
    scopes: "",
    accountLabel: "",
  };
}

export function RuntimeSlotsSection({
  projectId,
  onOpenProject,
  gateway: gatewayProp,
}: RuntimeSlotsSectionProps) {
  const [gateway] = useState<RuntimeSlotsGateway>(
    () => gatewayProp ?? defaultSlotsGateway(),
  );
  const [slots, setSlots] = useState<RuntimeSlotView[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [probe, setProbe] = useState<ProbeResult | null>(null);

  const [formMode, setFormMode] = useState<FormMode>("closed");
  const [editingSlotId, setEditingSlotId] = useState<string | null>(null);
  /** Create: provider picker first; connect step shows that provider's methods. */
  const [formStep, setFormStep] = useState<SlotFormStep>("provider");
  const [connectProviderId, setConnectProviderId] = useState<string | null>(
    null,
  );
  /** Auth kind after a provider is chosen — never the global four-tab list. */
  const [createAuthKind, setCreateAuthKind] = useState<SlotAuthKind>("api_key");
  const [createRole, setCreateRole] = useState<RuntimeSlotRole>("controller");
  const [roleCustomName, setRoleCustomName] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [modelId, setModelId] = useState("");
  /** Draft only — never persisted; cleared after successful save. */
  const [apiKeyDraft, setApiKeyDraft] = useState("");
  const [executablePath, setExecutablePath] = useState("");
  const [launchArgsText, setLaunchArgsText] = useState("");
  const [statusArgsText, setStatusArgsText] = useState("");
  /**
   * API-key create: selected provider preset. Changing the preset fills
   * `baseUrl` (and optional suggested model). User edits to baseUrl stick
   * until the user picks a *different* preset — never re-applied on render.
   */
  const [providerPresetId, setProviderPresetId] = useState<ProviderPresetId>(
    DEFAULT_PROVIDER_PRESET_ID,
  );
  /**
   * Draft model catalog from host `probe_model_catalog` (create/edit API-key).
   * Renderer never opens third-party sockets; key is input-only.
   */
  const [draftCatalog, setDraftCatalog] = useState<ProbeModelCatalogView | null>(
    null,
  );
  const [draftCatalogBusy, setDraftCatalogBusy] = useState(false);
  const [draftCatalogError, setDraftCatalogError] = useState<string | null>(
    null,
  );
  /** Local CLI detection results for create form. */
  const [detectRows, setDetectRows] = useState<LocalCliDetectionView[]>([]);
  const [detectBusy, setDetectBusy] = useState(false);
  const [detectError, setDetectError] = useState<string | null>(null);
  const [detectSelectedKind, setDetectSelectedKind] = useState<string | null>(
    null,
  );

  /** OAuth create: provider catalog + user-supplied public fields. */
  const [oauthCatalog, setOauthCatalog] = useState<ProviderLoginCatalogEntry[]>(
    [],
  );
  const [oauthCatalogBusy, setOauthCatalogBusy] = useState(false);
  const [oauthCatalogError, setOauthCatalogError] = useState<string | null>(
    null,
  );
  const [oauthProviderId, setOauthProviderId] = useState<string>("");
  const [oauthDraftFields, setOauthDraftFields] = useState<OauthDraftFields>(
    emptyOauthDraftFields,
  );
  /** True while host is performing browser OAuth (no optimistic slot). */
  const [oauthLoginBusy, setOauthLoginBusy] = useState(false);
  /** Public authorize URL from host progress / browser-open failure (copyable). */
  const [oauthAuthorizeUrl, setOauthAuthorizeUrl] = useState<string | null>(
    null,
  );
  /** False when host reported browser open failed (still may be waiting). */
  const [oauthBrowserOpened, setOauthBrowserOpened] = useState(true);
  /** Device-code user_code when host reports one (never the secret device_code). */
  const [oauthUserCode, setOauthUserCode] = useState<string | null>(null);

  /** Inline role editor for an existing list row. */
  const [roleEditSlotId, setRoleEditSlotId] = useState<string | null>(null);
  const [roleEditRole, setRoleEditRole] = useState<RuntimeSlotRole>("custom");
  const [roleEditCustomName, setRoleEditCustomName] = useState("");

  const selectedProvider = useMemo(
    () => getProviderPreset(providerPresetId),
    [providerPresetId],
  );
  const selectedConnectProvider = useMemo(
    () =>
      connectProviderId ? getSlotConnectProvider(connectProviderId) ?? null : null,
    [connectProviderId],
  );
  const availableAuthMethods = useMemo(() => {
    if (!selectedConnectProvider) return [];
    return connectProviderMethods(selectedConnectProvider, oauthCatalog);
  }, [oauthCatalog, selectedConnectProvider]);

  const hasController = useMemo(
    () => slots.some((slot) => slot.role === "controller"),
    [slots],
  );
  const hasLibrarian = useMemo(
    () => slots.some((slot) => slot.role === "librarian"),
    [slots],
  );
  const controllerAllowedForCreate = authAllowsController(createAuthKind);

  const selectedOauthProvider = useMemo(
    () =>
      oauthCatalog.find((entry) => entry.providerId === oauthProviderId)
      ?? null,
    [oauthCatalog, oauthProviderId],
  );
  const oauthRequiredFields = useMemo(
    () =>
      selectedOauthProvider
        ? oauthFieldsRequiringUserInput(selectedOauthProvider)
        : [],
    [selectedOauthProvider],
  );

  const reload = useCallback(async () => {
    if (!projectId) {
      setSlots([]);
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const list = await gateway.listRuntimeSlots(projectId);
      setSlots(list);
    } catch (cause) {
      // Never log the caught value — host errors may carry secrets.
      const message =
        cause instanceof Error ? cause.message : "加载运行时槽位失败。";
      setError(message);
    } finally {
      setLoading(false);
    }
  }, [gateway, projectId]);

  useEffect(() => {
    void reload();
  }, [reload]);

  function resetFormDrafts(): void {
    setDisplayName("");
    setBaseUrl("");
    setModelId("");
    setApiKeyDraft("");
    setExecutablePath("");
    setLaunchArgsText("");
    setStatusArgsText("");
    setEditingSlotId(null);
    setFormStep("provider");
    setConnectProviderId(null);
    setCreateAuthKind("api_key");
    setCreateRole("controller");
    setRoleCustomName("");
    setProviderPresetId(DEFAULT_PROVIDER_PRESET_ID);
    setDraftCatalog(null);
    setDraftCatalogBusy(false);
    setDraftCatalogError(null);
    setDetectRows([]);
    setDetectBusy(false);
    setDetectError(null);
    setDetectSelectedKind(null);
    setOauthCatalog([]);
    setOauthCatalogBusy(false);
    setOauthCatalogError(null);
    setOauthProviderId("");
    setOauthDraftFields(emptyOauthDraftFields());
    setOauthLoginBusy(false);
    setOauthAuthorizeUrl(null);
    setOauthBrowserOpened(true);
    setOauthUserCode(null);
  }

  async function loadOauthCatalog(): Promise<void> {
    setOauthCatalogBusy(true);
    setOauthCatalogError(null);
    try {
      const rows = await gateway.listProviderLoginCatalog();
      setOauthCatalog(rows);
      if (
        oauthProviderId
        && !rows.some((entry) => entry.providerId === oauthProviderId)
      ) {
        setOauthProviderId("");
      }
    } catch (cause) {
      const message =
        cause instanceof RuntimeSlotsGatewayError
          ? cause.message
          : "无法加载 OAuth 提供方目录。";
      setOauthCatalogError(message);
      setOauthCatalog([]);
      setOauthProviderId("");
    } finally {
      setOauthCatalogBusy(false);
    }
  }

  /**
   * Apply a provider preset: sets baseUrl from catalog and optional suggested
   * model. Only runs on explicit user selection (not on re-render).
   * Selecting the same id again re-applies defaults (intentional "reset to
   * preset"); switching away after a manual baseUrl edit is the only way a
   * different preset overwrites — user edits are never auto-clobbered.
   */
  function selectProviderPreset(nextId: ProviderPresetId): void {
    const preset = getProviderPreset(nextId);
    if (!preset) return;
    setProviderPresetId(nextId);
    // Preset fills baseUrl only when this selection changes the id OR user
    // explicitly re-picks. Callers pass nextId from the select onChange, which
    // fires when the value changes — so user-edited baseUrl is safe until
    // they choose another preset.
    setBaseUrl(preset.baseUrl);
    if (preset.suggestedModelId) {
      setModelId(preset.suggestedModelId);
    }
    // Stale catalog from a previous endpoint must not look like a success.
    setDraftCatalog(null);
    setDraftCatalogError(null);
  }

  async function handleDetectLocalClis(): Promise<void> {
    if (detectBusy || busy) return;
    setDetectBusy(true);
    setDetectError(null);
    setDetectSelectedKind(null);
    try {
      const rows = await gateway.detectLocalClis();
      setDetectRows(rows);
      if (!rows.some((row) => row.found)) {
        setDetectError(LOCAL_CLI_DETECT_NONE_FOUND);
      }
    } catch (cause) {
      const message =
        cause instanceof RuntimeSlotsGatewayError
          ? cause.message
          : "扫描本机 CLI 失败。";
      setDetectError(message);
      setDetectRows([]);
    } finally {
      setDetectBusy(false);
    }
  }

  function selectDetectedCli(row: LocalCliDetectionView): void {
    if (!row.found || !row.executablePath) return;
    setDetectSelectedKind(row.kind);
    setExecutablePath(row.executablePath);
    const defaults = LOCAL_CLI_DEFAULT_STATUS_ARGS[row.kind];
    if (defaults) {
      setStatusArgsText(defaults);
    }
    if (!displayName.trim()) {
      setDisplayName(row.kind);
    }
  }

  async function handleProbeDraftCatalog(): Promise<void> {
    if (draftCatalogBusy || busy) return;
    const url = baseUrl.trim();
    const key = apiKeyDraft.trim();
    if (!url || !key) {
      setDraftCatalogError("请先填写端点 base_url 与 API key，再拉取模型列表。");
      setDraftCatalog(null);
      return;
    }
    setDraftCatalogBusy(true);
    setDraftCatalogError(null);
    setDraftCatalog(null);
    try {
      // Key stays in draft input only; host never returns it. Do not clear
      // the draft on probe — user still needs it for create/save.
      const catalog = await gateway.probeModelCatalog(url, key);
      setDraftCatalog(catalog);
      if (catalog.status === "unsupported") {
        // Honest unsupported — do not invent a model id.
        setDraftCatalogError(null);
      }
    } catch (cause) {
      const message =
        cause instanceof RuntimeSlotsGatewayError
          ? cause.message
          : mapRuntimeSlotErrorMessage("runtime_slot_model_catalog_unavailable");
      setDraftCatalogError(message);
      setDraftCatalog(null);
      // Never auto-fill a model on failure.
    } finally {
      setDraftCatalogBusy(false);
    }
  }

  function openCreate(): void {
    resetFormDrafts();
    // Suggest controller only when the project has none yet.
    setCreateRole(hasController ? "custom" : "controller");
    setFormMode("create");
    setFormStep("provider");
    setRoleEditSlotId(null);
    setNotice(null);
    setError(null);
    setProbe(null);
    void loadOauthCatalog();
  }

  function applyConnectProviderDefaults(provider: SlotConnectProvider): void {
    if (provider.presetId) {
      selectProviderPreset(provider.presetId);
      return;
    }
    const catalogId = provider.catalogProviderId;
    const defaults = catalogId ? CATALOG_API_DEFAULTS[catalogId] : undefined;
    if (defaults) {
      setProviderPresetId(DEFAULT_PROVIDER_PRESET_ID);
      setBaseUrl(defaults.baseUrl);
      setModelId(defaults.suggestedModelId);
      setDraftCatalog(null);
      setDraftCatalogError(null);
      return;
    }
    setProviderPresetId(DEFAULT_PROVIDER_PRESET_ID);
    setBaseUrl("");
    setModelId("");
    setDraftCatalog(null);
    setDraftCatalogError(null);
  }

  function selectConnectProvider(providerId: string): void {
    const provider = getSlotConnectProvider(providerId);
    if (!provider) return;
    setConnectProviderId(provider.id);
    applyConnectProviderDefaults(provider);
    if (provider.catalogProviderId) {
      setOauthProviderId(provider.catalogProviderId);
      setOauthDraftFields(emptyOauthDraftFields());
    } else {
      setOauthProviderId("");
    }
    const methods = connectProviderMethods(provider, oauthCatalog);
    const nextAuth = methods[0] ?? "api_key";
    setCreateAuthKind(nextAuth);
    if (!authAllowsController(nextAuth) && createRole === "controller") {
      setCreateRole("custom");
    }
    setFormStep("connect");
    setError(null);
  }

  function openEdit(slot: RuntimeSlotView): void {
    if (slot.kind !== "governed_grok" || slot.authMode !== "api_key") {
      return;
    }
    const providerId = inferConnectProviderId({
      kind: slot.kind,
      authMode: slot.authMode,
      baseUrl: slot.baseUrl,
    });
    setFormMode("edit");
    setFormStep("connect");
    setEditingSlotId(slot.slotId);
    setConnectProviderId(providerId);
    setCreateAuthKind("api_key");
    setCreateRole(slot.role);
    setRoleCustomName(slot.roleCustomName ?? "");
    setDisplayName(slot.displayName);
    setBaseUrl(slot.baseUrl ?? "");
    setModelId(slot.modelId ?? "");
    // Leave secret field blank = keep stored value.
    setApiKeyDraft("");
    // Edit keeps the stored baseUrl; do not re-apply a preset over it.
    const inferred = getSlotConnectProvider(providerId);
    setProviderPresetId(inferred?.presetId ?? DEFAULT_PROVIDER_PRESET_ID);
    setRoleEditSlotId(null);
    setNotice(null);
    setError(null);
    setProbe(null);
  }

  function closeForm(): void {
    setFormMode("closed");
    resetFormDrafts();
  }

  function selectCreateAuth(kind: SlotAuthKind): void {
    if (!availableAuthMethods.includes(kind)) return;
    setCreateAuthKind(kind);
    if (!authAllowsController(kind) && createRole === "controller") {
      setCreateRole("custom");
    }
  }

  function setOauthField(fieldId: OauthUserFieldId, value: string): void {
    setOauthDraftFields((prev) => ({ ...prev, [fieldId]: value }));
  }

  function buildOauthSupplement(): OauthLoginSupplementInput {
    const supplement: OauthLoginSupplementInput = {};
    const clientId = oauthDraftFields.clientId.trim();
    const issuer = oauthDraftFields.issuer.trim();
    const authorizeEndpoint = oauthDraftFields.authorizeEndpoint.trim();
    const deviceAuthorizationEndpoint =
      oauthDraftFields.deviceAuthorizationEndpoint.trim();
    const tokenEndpoint = oauthDraftFields.tokenEndpoint.trim();
    const scopesText = oauthDraftFields.scopes.trim();
    const accountLabel = oauthDraftFields.accountLabel.trim();
    if (clientId) supplement.clientId = clientId;
    if (issuer) supplement.issuer = issuer;
    if (authorizeEndpoint) supplement.authorizeEndpoint = authorizeEndpoint;
    if (deviceAuthorizationEndpoint) {
      supplement.deviceAuthorizationEndpoint = deviceAuthorizationEndpoint;
    }
    if (tokenEndpoint) supplement.tokenEndpoint = tokenEndpoint;
    if (scopesText) {
      supplement.scopes = scopesText.split(/\s+/u).filter(Boolean);
    }
    if (accountLabel) supplement.accountLabel = accountLabel;
    return supplement;
  }

  function openRoleEdit(slot: RuntimeSlotView): void {
    setRoleEditSlotId(slot.slotId);
    setRoleEditRole(slot.role);
    setRoleEditCustomName(slot.roleCustomName ?? "");
    setFormMode("closed");
    resetFormDrafts();
    setError(null);
    setNotice(null);
  }

  function closeRoleEdit(): void {
    setRoleEditSlotId(null);
    setRoleEditRole("custom");
    setRoleEditCustomName("");
  }

  async function handleSave(event: FormEvent): Promise<void> {
    event.preventDefault();
    if (!projectId || busy) return;

    const name = displayName.trim();
    if (!name) {
      setError("显示名称不能为空。");
      return;
    }

    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      if (formMode === "edit" && editingSlotId) {
        const editKey = apiKeyDraft;
        // Clear draft before host call so failures never leave key material in DOM.
        setApiKeyDraft("");
        await gateway.updateGrokApiKeySlot({
          projectId,
          slotId: editingSlotId,
          displayName: name,
          baseUrl: baseUrl.trim(),
          modelId: modelId.trim(),
          apiKey: editKey,
        });
        // Role may have changed in the same form.
        const editing = slots.find((slot) => slot.slotId === editingSlotId);
        if (
          editing
          && (createRole !== editing.role
            || (createRole === "custom"
              && (roleCustomName.trim() || "未分配")
                !== (editing.roleCustomName ?? "未分配")))
        ) {
          if (createRole === "controller" && !slotAllowsController(editing)) {
            setError(CONTROLLER_REQUIRES_GOVERNED_AUTH_HINT);
            setBusy(false);
            return;
          }
          if (createRole === "custom" && !roleCustomName.trim()) {
            setError("自定义角色需要填写名称。");
            setBusy(false);
            return;
          }
          await gateway.updateSlot({
            projectId,
            slotId: editingSlotId,
            displayName: name,
            role: createRole,
            roleCustomName:
              createRole === "custom" ? roleCustomName.trim() : undefined,
            keepAuth: true,
          });
        }
        setNotice("槽位已更新。");
      } else {
        // Create: single gateway.createSlot call.
        if (createRole === "controller" && !controllerAllowedForCreate) {
          setError(CONTROLLER_REQUIRES_GOVERNED_AUTH_HINT);
          setBusy(false);
          return;
        }
        if (createRole === "custom" && !roleCustomName.trim()) {
          setError("自定义角色需要填写名称。");
          setBusy(false);
          return;
        }

        if (createAuthKind === "api_key") {
          const key = apiKeyDraft.trim();
          if (!key) {
            setError("创建 API-key 槽位时必须填写 API key。");
            setBusy(false);
            return;
          }
          if (!baseUrl.trim() || !modelId.trim()) {
            setError("请填写端点 base_url 与模型 id。");
            setBusy(false);
            return;
          }
          // Clear draft before await so a host failure never leaves key material in DOM.
          setApiKeyDraft("");
          await gateway.createSlot({
            projectId,
            displayName: name,
            authKind: "api_key",
            role: createRole,
            roleCustomName:
              createRole === "custom" ? roleCustomName.trim() : undefined,
            baseUrl: baseUrl.trim(),
            modelId: modelId.trim(),
            apiKey: key,
          });
          setNotice(
            "已创建槽位。密钥已交由宿主保管，界面仅显示「已配置」。",
          );
        } else if (createAuthKind === "grok_login") {
          await gateway.createSlot({
            projectId,
            displayName: name,
            authKind: "grok_login",
            role: createRole,
            roleCustomName:
              createRole === "custom" ? roleCustomName.trim() : undefined,
          });
          // Say what was actually checked. The old copy claimed success for a
          // row that had been written without verifying anything, which is
          // how a slot with no model and a red catalog could read as "created".
          setNotice(
            "已创建槽位：已确认本机 Grok CLI 已登录，并填入其默认模型。可直接点「启动」。",
          );
        } else if (createAuthKind === "oauth") {
          if (!selectedOauthProvider) {
            setError("请选择 OAuth 提供方。");
            setBusy(false);
            return;
          }
          if (!baseUrl.trim() || !modelId.trim()) {
            setError("请填写端点 base_url 与模型 id。");
            setBusy(false);
            return;
          }
          for (const field of oauthRequiredFields) {
            if (!oauthDraftFields[field.fieldId].trim()) {
              setError(`请填写 ${field.label}：${field.reason}`);
              setBusy(false);
              return;
            }
          }
          // In-progress only — never optimistically add a slot before host OK.
          setOauthLoginBusy(true);
          setOauthAuthorizeUrl(null);
          setOauthBrowserOpened(true);
          setOauthUserCode(null);
          let unlistenProgress: (() => void) | null = null;
          try {
            // Subscribe before invoke so mid-login progress is not missed.
            // Only in a real Tauri shell — vitest/mock has no event bus.
            const inTauri =
              typeof window !== "undefined"
              && ("__TAURI_INTERNALS__" in window || "__TAURI__" in window);
            if (inTauri) {
              try {
                const { listen } = await import("@tauri-apps/api/event");
                unlistenProgress = await listen(
                  RUNTIME_SLOT_OAUTH_PROGRESS_EVENT,
                  (event) => {
                    const parsed = parseOauthProgressPayload(event.payload);
                    if (!parsed) return;
                    setOauthAuthorizeUrl(parsed.authorizeUrl);
                    setOauthBrowserOpened(parsed.browserOpened);
                    setOauthUserCode(parsed.userCode);
                  },
                );
              } catch {
                unlistenProgress = null;
              }
            }
            await gateway.createSlot({
              projectId,
              displayName: name,
              authKind: "oauth",
              role: createRole,
              roleCustomName:
                createRole === "custom" ? roleCustomName.trim() : undefined,
              baseUrl: baseUrl.trim(),
              modelId: modelId.trim(),
              providerId: selectedOauthProvider.providerId,
              oauthSupplement: buildOauthSupplement(),
            });
            setOauthAuthorizeUrl(null);
            setOauthUserCode(null);
            setNotice(
              "已通过 OAuth 创建槽位。令牌由宿主保管，界面仅显示账号标识与过期时间。",
            );
          } finally {
            if (unlistenProgress) {
              try {
                unlistenProgress();
              } catch {
                // ignore unlisten errors
              }
            }
            setOauthLoginBusy(false);
          }
        } else {
          const statusArgs = parseArgvField(statusArgsText);
          if (!executablePath.trim()) {
            setError("请填写可执行文件的绝对路径。");
            setBusy(false);
            return;
          }
          if (statusArgs.length === 0) {
            setError("请填写至少一项账号状态命令参数（例如 login status）。");
            setBusy(false);
            return;
          }
          await gateway.createSlot({
            projectId,
            displayName: name,
            authKind: "local_cli",
            role: createRole,
            roleCustomName:
              createRole === "custom" ? roleCustomName.trim() : undefined,
            executablePath: executablePath.trim(),
            launchArgs: parseArgvField(launchArgsText),
            statusArgs,
          });
          setNotice("已创建槽位（本机 CLI，不受治理）。");
        }
      }
      closeForm();
      await reload();
    } catch (cause) {
      // Failure: form stays open only if we did not close; ensure no slot paint.
      setOauthLoginBusy(false);
      setError(mapSaveError(cause));
      const fallbackUrl = authorizeUrlFromError(cause);
      if (fallbackUrl) {
        setOauthAuthorizeUrl(fallbackUrl);
        setOauthBrowserOpened(false);
      }
    } finally {
      setBusy(false);
      setOauthLoginBusy(false);
    }
  }

  async function handleRoleSave(slot: RuntimeSlotView): Promise<void> {
    if (!projectId || busy) return;
    if (roleEditRole === "controller" && !slotAllowsController(slot)) {
      setError(CONTROLLER_REQUIRES_GOVERNED_AUTH_HINT);
      return;
    }
    if (roleEditRole === "custom" && !roleEditCustomName.trim()) {
      setError("自定义角色需要填写名称。");
      return;
    }
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await gateway.updateSlot({
        projectId,
        slotId: slot.slotId,
        displayName: slot.displayName,
        role: roleEditRole,
        roleCustomName:
          roleEditRole === "custom" ? roleEditCustomName.trim() : undefined,
        keepAuth: true,
      });
      closeRoleEdit();
      setNotice("角色已更新。");
      await reload();
    } catch (cause) {
      setError(mapSaveError(cause));
    } finally {
      setBusy(false);
    }
  }

  async function handleProbe(slot: RuntimeSlotView): Promise<void> {
    if (!projectId || busy) return;
    setBusy(true);
    setError(null);
    setNotice(null);
    setProbe(null);
    try {
      if (slot.kind === "external_cli") {
        const status = await gateway.getExternalCliStatus(
          projectId,
          slot.slotId,
        );
        const classified = classifyExternalCliStatus(status);
        const raw = [status.stdout, status.stderr]
          .map((part) => part.trim())
          .filter(Boolean)
          .join("\n");
        setProbe({
          slotId: slot.slotId,
          ok: classified.ok,
          text: raw
            ? `${classified.summary}\n${raw}`
            : classified.summary,
        });
      } else {
        const started = await gateway.startRuntimeSlot(
          projectId,
          slot.slotId,
        );
        // Host is the only source of which slot is live — re-list after start.
        const list = await gateway.listRuntimeSlots(projectId);
        setSlots(list);
        setProbe({
          slotId: slot.slotId,
          ok: true,
          text: `探活成功：槽位已激活（${started.status}）。`,
        });
      }
    } catch (cause) {
      const code =
        cause instanceof RuntimeSlotsGatewayError
          ? cause.code
          : "runtime_slots_command_failed";
      const message =
        slot.kind === "governed_grok"
          ? mapGrokProbeFailureMessage(code)
          : cause instanceof Error
            ? cause.message
            : "探活失败。";
      setProbe({
        slotId: slot.slotId,
        ok: false,
        text: message,
      });
    } finally {
      setBusy(false);
    }
  }

  /**
   * After quick model switch: update list modelId from host result, then
   * reload for full consistency. Switch notice lives on the picker itself
   * (runtimeRestarted honesty).
   */
  async function handleModelSwitched(
    result: RuntimeSlotModelSwitchView,
  ): Promise<void> {
    setSlots((prev) =>
      prev.map((slot) =>
        slot.slotId === result.slotId
          ? { ...slot, modelId: result.modelId }
          : slot
      ),
    );
    // Quiet reload — picker already showed restart/save outcome.
    try {
      if (projectId) {
        const list = await gateway.listRuntimeSlots(projectId);
        setSlots(list);
      }
    } catch {
      // Keep optimistic modelId; list errors surface on next full reload.
    }
  }

  if (!projectId) {
    return (
      <section
        className="runtime-slots"
        data-testid="runtime-slots-section"
      >
        <div className="runtime-slots__head">
          <h3>运行时槽位</h3>
          <p>打开项目后可新增与配置项目级运行时槽位。</p>
        </div>
        <p
          className="runtime-slots__hint"
          data-testid="runtime-slots-no-project"
          role="status"
        >
          需要先打开项目：槽位按项目隔离，不能跨项目复用。
        </p>
        {/*
          Telling someone a prerequisite without offering it is a dead end: this
          was the first place a real session got stuck looking for "新增".
          Settings stays open, so the section fills in as soon as a project is
          picked.
        */}
        {onOpenProject ? (
          <button
            className="runtime-slots__open-project"
            data-testid="runtime-slots-open-project"
            onClick={onOpenProject}
            type="button"
          >
            打开项目
          </button>
        ) : null}
      </section>
    );
  }

  return (
    <section
      className="runtime-slots"
      data-testid="runtime-slots-section"
    >
      <div className="runtime-slots__head">
        <h3>运行时槽位</h3>
        <p>
          先选供应商，再只显示该供应商真正支持的认证方式。
          槽位可以配置多个并长期保留；同一时刻有一个槽位在对话，
          总控通过派单让其他角色干活，每一步仍走批准与收据。
          API key 仅在提交时发送给宿主，保存后界面只显示「已配置」。
        </p>
      </div>

      {/* T3: distinguish zero slots vs slots-without-controller. */}
      {!loading && slots.length === 0 ? (
        <p
          className="runtime-slots-ungoverned"
          data-testid="runtime-slots-guidance-no-slots"
          role="status"
        >
          <strong>{GUIDANCE_NO_SLOTS_HEADING}</strong>
          。{GUIDANCE_NO_SLOTS_BODY}
        </p>
      ) : null}
      {!loading && slots.length > 0 && !hasController ? (
        <p
          className="runtime-slots-ungoverned"
          data-testid="runtime-slots-guidance-no-controller"
          role="status"
        >
          <strong>{GUIDANCE_NO_CONTROLLER_HEADING}</strong>
          。{GUIDANCE_NO_CONTROLLER_BODY}
        </p>
      ) : null}
      {!loading && hasController && !hasLibrarian ? (
        <p
          className="runtime-slots__hint"
          data-testid="runtime-slots-guidance-librarian"
          role="note"
        >
          {GUIDANCE_LIBRARIAN_SUGGESTION}
        </p>
      ) : null}

      {loading ? (
        <p className="runtime-slots__status">正在加载槽位…</p>
      ) : slots.length === 0 ? (
        <p
          className="runtime-slots__hint"
          data-testid="runtime-slots-empty"
        >
          点击下方「新建槽位」开始配置。
        </p>
      ) : (
        <ul className="runtime-slots-list" data-testid="runtime-slots-list">
          {slots.map((slot) => (
            <li
              className={
                slot.isActive
                  ? "runtime-slots-list__item runtime-slots-list__item--active"
                  : "runtime-slots-list__item"
              }
              data-active-runtime={slot.isActive ? "true" : "false"}
              data-slot-id={slot.slotId}
              data-testid={`runtime-slot-${slot.slotId}`}
              key={slot.slotId}
            >
              <div className="runtime-slots-list__row">
                <div className="runtime-slots-list__meta">
                  <span className="runtime-slots-list__name">
                    {slot.displayName}
                  </span>
                  {slot.governance === "full" ? (
                    <span
                      className="runtime-slots-badge runtime-slots-badge--governed"
                      data-testid={`runtime-slot-governed-${slot.slotId}`}
                    >
                      {GOVERNED_GROK_BADGE}
                    </span>
                  ) : (
                    <span
                      className="runtime-slots-badge runtime-slots-badge--ungoverned"
                      data-testid={`runtime-slot-ungoverned-badge-${slot.slotId}`}
                    >
                      不受治理
                    </span>
                  )}
                  <span className="runtime-slots-badge">
                    {authModeBadge(slot)}
                  </span>
                  <span
                    className="runtime-slots-badge"
                    data-testid={`runtime-slot-role-${slot.slotId}`}
                  >
                    {roleLabel(slot)}
                  </span>
                  {slot.kind === "governed_grok"
                  && slot.governance === "full" ? (
                    slot.isActive ? (
                      <span
                        className="runtime-slots-badge runtime-slots-badge--started"
                        data-testid={`runtime-slot-started-${slot.slotId}`}
                      >
                        已启动
                      </span>
                    ) : (
                      <span
                        className="runtime-slots-badge runtime-slots-badge--needs-start"
                        data-testid={`runtime-slot-needs-start-${slot.slotId}`}
                      >
                        未启动 · 需启动后才能对话
                      </span>
                    )
                  ) : null}
                </div>
                <div className="runtime-slots-list__actions">
                  <button
                    data-testid={`runtime-slot-role-edit-${slot.slotId}`}
                    disabled={busy}
                    onClick={() => openRoleEdit(slot)}
                    type="button"
                  >
                    改角色
                  </button>
                  {slot.kind === "governed_grok"
                  && slot.authMode === "api_key" ? (
                    <button
                      data-testid={`runtime-slot-edit-${slot.slotId}`}
                      disabled={busy}
                      onClick={() => openEdit(slot)}
                      type="button"
                    >
                      编辑
                    </button>
                  ) : null}
                  {slot.kind === "external_cli" ? (
                    <button
                      data-testid={`runtime-slot-probe-${slot.slotId}`}
                      disabled={busy}
                      onClick={() => void handleProbe(slot)}
                      type="button"
                    >
                      查询账号状态
                    </button>
                  ) : (
                    <button
                      className={
                        slot.isActive
                          ? "runtime-slots-start"
                          : "runtime-slots-start runtime-slots-start--primary"
                      }
                      data-testid={`runtime-slot-probe-${slot.slotId}`}
                      disabled={busy}
                      onClick={() => void handleProbe(slot)}
                      type="button"
                    >
                      {slot.isActive ? "重新启动" : "启动"}
                    </button>
                  )}
                </div>
              </div>

              {roleEditSlotId === slot.slotId ? (
                <div
                  className="runtime-slots-form"
                  data-testid={`runtime-slot-role-form-${slot.slotId}`}
                >
                  <p className="runtime-slots-form__title">更改角色</p>
                  <div
                    className="runtime-slots-form__kind"
                    role="group"
                    aria-label="槽位角色"
                  >
                    {SLOT_ROLE_OPTIONS.map((option) => {
                      const disabled =
                        option.id === "controller"
                        && !slotAllowsController(slot);
                      return (
                        <button
                          aria-pressed={roleEditRole === option.id}
                          data-testid={
                            `runtime-slot-role-option-${slot.slotId}-${option.id}`
                          }
                          disabled={disabled || busy}
                          key={option.id}
                          onClick={() => setRoleEditRole(option.id)}
                          title={
                            disabled
                              ? CONTROLLER_REQUIRES_GOVERNED_AUTH_HINT
                              : option.description
                          }
                          type="button"
                        >
                          {option.label}
                        </button>
                      );
                    })}
                  </div>
                  {!slotAllowsController(slot) ? (
                    <p
                      className="runtime-slots__hint"
                      data-testid={
                        `runtime-slot-controller-cli-hint-${slot.slotId}`
                      }
                      role="note"
                    >
                      {CONTROLLER_REQUIRES_GOVERNED_AUTH_HINT}
                    </p>
                  ) : null}
                  {roleEditRole === "custom" ? (
                    <label className="settings-field">
                      <span className="settings-field__label">自定义角色名</span>
                      <input
                        aria-label="自定义角色名"
                        autoComplete="off"
                        className="settings-field__control"
                        data-testid={
                          `runtime-slot-role-custom-name-${slot.slotId}`
                        }
                        disabled={busy}
                        onChange={(event) =>
                          setRoleEditCustomName(event.target.value)}
                        spellCheck={false}
                        type="text"
                        value={roleEditCustomName}
                      />
                    </label>
                  ) : null}
                  <div className="settings-actions modal-actions">
                    <button
                      data-testid={
                        `runtime-slot-role-cancel-${slot.slotId}`
                      }
                      disabled={busy}
                      onClick={closeRoleEdit}
                      type="button"
                    >
                      取消
                    </button>
                    <button
                      data-testid={`runtime-slot-role-save-${slot.slotId}`}
                      disabled={busy}
                      onClick={() => void handleRoleSave(slot)}
                      type="button"
                    >
                      {busy ? "保存中…" : "保存角色"}
                    </button>
                  </div>
                </div>
              ) : null}

              {slot.governance === "none" ? (
                <p
                  className="runtime-slots-ungoverned"
                  data-testid={`runtime-slot-ungoverned-${slot.slotId}`}
                  role="note"
                >
                  <strong>{EXTERNAL_CLI_UNGOVERNED_HEADING}</strong>
                  。{EXTERNAL_CLI_UNGOVERNED_BODY}
                </p>
              ) : null}

              {slot.kind === "governed_grok" && slot.authMode === "api_key" ? (
                <p className="runtime-slots-list__detail">
                  端点：{slot.baseUrl ?? "—"}
                  {" · "}
                  密钥：
                  <span data-testid={`runtime-slot-secret-status-${slot.slotId}`}>
                    {slot.secretConfigured
                      ? SECRET_CONFIGURED_LABEL
                      : SECRET_NOT_CONFIGURED_LABEL}
                  </span>
                </p>
              ) : null}

              {slot.kind === "governed_grok" && slot.authMode === "oauth" ? (
                <p
                  className="runtime-slots-list__detail"
                  data-testid={`runtime-slot-oauth-status-${slot.slotId}`}
                >
                  端点：{slot.baseUrl ?? "—"}
                  {" · "}
                  账号：
                  <span data-testid={`runtime-slot-oauth-account-${slot.slotId}`}>
                    {slot.accountLabel?.trim() || "—"}
                  </span>
                  {" · "}
                  过期：
                  <span data-testid={`runtime-slot-oauth-expiry-${slot.slotId}`}>
                    {formatOauthExpiryForDisplay(slot.accessExpiresAt)}
                  </span>
                </p>
              ) : null}

              {/*
                Quick model switch on the list row — core of "each slot like
                desktop model switch". external_cli must not get a picker.
              */}
              {slot.kind === "governed_grok" && projectId ? (
                <RuntimeSlotModelPicker
                  currentModelId={slot.modelId}
                  disabled={busy}
                  gateway={gateway}
                  onSwitched={(result) => {
                    void handleModelSwitched(result);
                  }}
                  projectId={projectId}
                  slotId={slot.slotId}
                />
              ) : null}

              {slot.kind === "external_cli" ? (
                <p className="runtime-slots-list__detail">
                  可执行文件：{slot.executablePath ?? "—"}
                  {slot.statusArgs && slot.statusArgs.length > 0
                    ? ` · 状态参数：${slot.statusArgs.join(" ")}`
                    : null}
                </p>
              ) : null}

              {probe && probe.slotId === slot.slotId ? (
                <p
                  className={
                    probe.ok
                      ? "runtime-slots-probe runtime-slots-probe--ok"
                      : "runtime-slots-probe runtime-slots-probe--fail"
                  }
                  data-testid={`runtime-slot-probe-result-${slot.slotId}`}
                  role={probe.ok ? "status" : "alert"}
                >
                  {probe.text}
                </p>
              ) : null}
            </li>
          ))}
        </ul>
      )}

      {formMode === "closed" ? (
        <button
          className="runtime-slots__add"
          data-testid="runtime-slots-add"
          disabled={busy || loading}
          onClick={openCreate}
          type="button"
        >
          新建槽位
        </button>
      ) : (
        <form
          className="runtime-slots-form"
          data-testid="runtime-slots-form"
          onSubmit={(event) => {
            if (formMode === "create" && formStep === "provider") {
              event.preventDefault();
              return;
            }
            void handleSave(event);
          }}
        >
          <p className="runtime-slots-form__title">
            {formMode === "edit"
              ? "编辑槽位"
              : formStep === "provider"
              ? "新建槽位 · 选择供应商"
              : `新建槽位 · ${selectedConnectProvider?.label ?? "连接"}`}
          </p>
          <span hidden data-testid="runtime-slots-form-step">
            {formMode === "edit" ? "connect" : formStep}
          </span>

          <label className="settings-field">
            <span className="settings-field__label">名称</span>
            <input
              aria-label="槽位显示名称"
              autoComplete="off"
              className="settings-field__control"
              data-testid="runtime-slots-display-name"
              disabled={busy}
              onChange={(event) => setDisplayName(event.target.value)}
              placeholder="例如：写码 Grok、总控"
              spellCheck={false}
              type="text"
              value={displayName}
            />
          </label>

          {formMode === "create" && formStep === "provider" ? (
            <div
              className="settings-provider-picker"
              data-testid="runtime-slots-provider-step"
            >
              {SLOT_CONNECT_PROVIDER_GROUPS.map((group) => {
                const rows = SLOT_CONNECT_PROVIDERS.filter(
                  (provider) => provider.group === group.id,
                );
                if (rows.length === 0) return null;
                return (
                  <div
                    className="settings-provider-picker__group"
                    key={group.id}
                  >
                    <p className="settings-provider-picker__heading">
                      {group.label}
                    </p>
                    <div
                      className="settings-provider-picker__grid"
                      role="list"
                    >
                      {rows.map((provider) => (
                        <button
                          data-testid={`runtime-slots-provider-pick-${provider.id}`}
                          disabled={busy}
                          key={provider.id}
                          onClick={() => selectConnectProvider(provider.id)}
                          role="listitem"
                          title={provider.description}
                          type="button"
                        >
                          <span>{provider.label}</span>
                          <span className="settings-provider-picker__desc">
                            {provider.description}
                          </span>
                        </button>
                      ))}
                    </div>
                  </div>
                );
              })}
            </div>
          ) : null}

          {formStep === "connect" || formMode === "edit" ? (
            <>
              {formMode === "create" ? (
                <div className="settings-provider-picker__current">
                  <button
                    data-testid="runtime-slots-form-back"
                    disabled={busy || oauthLoginBusy}
                    onClick={() => {
                      setFormStep("provider");
                      setError(null);
                    }}
                    type="button"
                  >
                    返回供应商
                  </button>
                  <span data-testid="runtime-slots-connect-provider">
                    {selectedConnectProvider?.label ?? connectProviderId}
                  </span>
                </div>
              ) : (
                <p
                  className="runtime-slots__hint"
                  data-testid="runtime-slots-connect-provider"
                  role="note"
                >
                  供应商：
                  {selectedConnectProvider?.label ?? connectProviderId ?? "自定义端点"}
                  （编辑时不可更换供应商或认证方式）
                </p>
              )}

              <div
                className="runtime-slots-form__kind"
                role="group"
                aria-label="认证方式"
                data-testid="runtime-slots-connect-step"
              >
                <span className="settings-field__label">认证方式</span>
                {SLOT_AUTH_METHODS.filter((method) =>
                  availableAuthMethods.includes(method.id),
                ).map((method) => (
                  <button
                    aria-pressed={createAuthKind === method.id}
                    data-testid={`runtime-slots-auth-${method.id}`}
                    disabled={busy || formMode === "edit"}
                    key={method.id}
                    onClick={() => selectCreateAuth(method.id)}
                    title={method.description}
                    type="button"
                  >
                    {method.label}
                  </button>
                ))}
              </div>
              <p
                className="runtime-slots__hint"
                data-testid="runtime-slots-auth-description"
                role="note"
              >
                {SLOT_AUTH_METHODS.find((method) => method.id === createAuthKind)
                  ?.description}
              </p>
            </>
          ) : null}

          {formMode === "create" && formStep === "connect" && createAuthKind === "local_cli" ? (
            <p
              className="runtime-slots-ungoverned"
              data-testid="runtime-slots-create-ungoverned-warning"
              role="note"
            >
              <strong>{EXTERNAL_CLI_UNGOVERNED_HEADING}</strong>
              。{EXTERNAL_CLI_UNGOVERNED_BODY}
            </p>
          ) : null}

          {(formMode === "edit" || formStep === "connect")
          && createAuthKind === "api_key" ? (
            <>
              {formMode === "create" ? (
                <div className="settings-field" data-testid="runtime-slots-provider-field">
                  <p
                    className="runtime-slots__hint"
                    data-testid="runtime-slots-provider-notes"
                    role="note"
                  >
                    {selectedConnectProvider?.description
                      ?? selectedProvider?.notes
                      ?? CATALOG_API_DEFAULTS[connectProviderId ?? ""]?.notes
                      ?? ""}
                    {selectedProvider?.docsUrl
                      || CATALOG_API_DEFAULTS[connectProviderId ?? ""]?.docsUrl
                      ? (
                        <>
                          {" "}
                          <a
                            data-testid="runtime-slots-provider-docs"
                            href={
                              selectedProvider?.docsUrl
                              || CATALOG_API_DEFAULTS[connectProviderId ?? ""]
                                ?.docsUrl
                              || ""
                            }
                            rel="noreferrer"
                            target="_blank"
                          >
                            获取 API key / 文档
                          </a>
                        </>
                      )
                      : null}
                  </p>
                  <span hidden data-testid="runtime-slots-provider">
                    {providerPresetId}
                  </span>
                </div>
              ) : null}

              <label className="settings-field">
                <span className="settings-field__label">
                  端点 base_url
                  {formMode === "create" ? (
                    <span className="runtime-slots-badge">
                      可手改
                    </span>
                  ) : null}
                </span>
                <input
                  aria-label="端点 base_url"
                  autoComplete="off"
                  className="settings-field__control"
                  data-testid="runtime-slots-base-url"
                  disabled={busy}
                  onChange={(event) => {
                    // User edit wins; preset is not re-applied until they
                    // pick a different provider in the select above.
                    setBaseUrl(event.target.value);
                  }}
                  placeholder="https://…"
                  spellCheck={false}
                  type="text"
                  value={baseUrl}
                />
              </label>

              <label className="settings-field">
                <span className="settings-field__label">
                  API key
                  {formMode === "edit" ? (
                    <span className="runtime-slots-badge">
                      留空则保持原值
                    </span>
                  ) : null}
                </span>
                <input
                  aria-label="API key"
                  autoComplete="new-password"
                  className="settings-field__control"
                  data-testid="runtime-slots-api-key"
                  disabled={busy}
                  onChange={(event) => setApiKeyDraft(event.target.value)}
                  placeholder={
                    formMode === "edit"
                      ? "留空表示不修改已保存的密钥"
                      : "输入后不回显；保存后仅显示「已配置」"
                  }
                  spellCheck={false}
                  type="password"
                  value={apiKeyDraft}
                />
              </label>

              <div
                className="runtime-slots-form__draft-catalog"
                data-testid="runtime-slots-draft-catalog"
              >
                <button
                  data-testid="runtime-slots-draft-catalog-pull"
                  disabled={
                    busy
                    || draftCatalogBusy
                    || !baseUrl.trim()
                    || !apiKeyDraft.trim()
                  }
                  onClick={() => void handleProbeDraftCatalog()}
                  type="button"
                >
                  {draftCatalogBusy ? "拉取中…" : "拉取模型列表"}
                </button>
                <p className="runtime-slots__hint" role="note">
                  由宿主请求端点的 /models；密钥不进入目录结果。
                </p>
                {draftCatalogBusy ? (
                  <p
                    className="runtime-slots__hint"
                    data-testid="runtime-slots-draft-models-loading"
                    role="status"
                  >
                    正在拉取模型目录…
                  </p>
                ) : null}
                {draftCatalog?.status === "available"
                  && draftCatalog.models.length > 0 ? (
                  <label className="settings-field">
                    <span className="settings-field__label">从目录选择</span>
                    <select
                      aria-label="草稿模型目录"
                      className="settings-field__control"
                      data-testid="runtime-slots-draft-models-select"
                      disabled={busy}
                      onChange={(event) => {
                        const value = event.target.value;
                        if (value) setModelId(value);
                      }}
                      value={
                        draftCatalog.models.includes(modelId) ? modelId : ""
                      }
                    >
                      <option disabled value="">
                        选择模型…
                      </option>
                      {draftCatalog.models.map((model) => (
                        <option key={model} title={model} value={model}>
                          {truncateModelIdForDisplay(model)}
                        </option>
                      ))}
                    </select>
                  </label>
                ) : null}
                {draftCatalog?.status === "unsupported" ? (
                  <p
                    className="runtime-slots__hint"
                    data-testid="runtime-slots-draft-models-unsupported"
                    role="status"
                  >
                    {mapModelCatalogUnsupportedReason(draftCatalog.reasonCode)
                      || MODEL_CATALOG_UNSUPPORTED_ENDPOINT}
                  </p>
                ) : null}
                {draftCatalogError ? (
                  <p
                    className="runtime-slots__error"
                    data-testid="runtime-slots-draft-models-error"
                    role="alert"
                  >
                    {draftCatalogError}
                  </p>
                ) : null}
              </div>

              <label className="settings-field">
                <span className="settings-field__label">模型 id</span>
                <input
                  aria-label="模型 id"
                  autoComplete="off"
                  className="settings-field__control"
                  data-testid="runtime-slots-model-id"
                  disabled={busy}
                  onChange={(event) => setModelId(event.target.value)}
                  placeholder="例如 deepseek-chat"
                  spellCheck={false}
                  type="text"
                  value={modelId}
                />
              </label>
            </>
          ) : null}

          {formMode === "create" && formStep === "connect" && createAuthKind === "local_cli" ? (
            <>
              <div
                className="runtime-slots-form__detect"
                data-testid="runtime-slots-detect"
              >
                <p className="runtime-slots__hint" role="note">
                  {LOCAL_CLI_DETECT_HINT}
                </p>
                <button
                  data-testid="runtime-slots-detect-scan"
                  disabled={busy || detectBusy}
                  onClick={() => void handleDetectLocalClis()}
                  type="button"
                >
                  {detectBusy ? LOCAL_CLI_DETECT_BUSY : "扫描本机 CLI"}
                </button>
                {detectError ? (
                  <p
                    className="runtime-slots__hint"
                    data-testid="runtime-slots-detect-error"
                    role="status"
                  >
                    {detectError}
                  </p>
                ) : null}
                {detectRows.length > 0 ? (
                  <ul
                    className="runtime-slots-detect-list"
                    data-testid="runtime-slots-detect-list"
                  >
                    {detectRows.map((row) => {
                      const authText = formatLocalCliAuthLabel(row);
                      const selectable = row.found && Boolean(row.executablePath);
                      return (
                        <li
                          data-testid={`runtime-slots-detect-row-${row.kind}`}
                          key={row.kind}
                        >
                          <button
                            aria-pressed={detectSelectedKind === row.kind}
                            data-testid={`runtime-slots-detect-select-${row.kind}`}
                            disabled={!selectable || busy}
                            onClick={() => selectDetectedCli(row)}
                            title={
                              selectable
                                ? `填入 ${row.executablePath}`
                                : "未安装，请手填路径"
                            }
                            type="button"
                          >
                            <span data-testid={`runtime-slots-detect-kind-${row.kind}`}>
                              {row.kind}
                            </span>
                            {row.found ? (
                              <span data-testid={`runtime-slots-detect-path-${row.kind}`}>
                                {row.executablePath}
                              </span>
                            ) : (
                              <span>未安装</span>
                            )}
                            {row.version ? (
                              <span data-testid={`runtime-slots-detect-version-${row.kind}`}>
                                {row.version}
                              </span>
                            ) : null}
                            <span
                              data-auth-status={row.authStatus}
                              data-testid={`runtime-slots-detect-auth-${row.kind}`}
                            >
                              {authText}
                            </span>
                          </button>
                        </li>
                      );
                    })}
                  </ul>
                ) : null}
              </div>
              <label className="settings-field">
                <span className="settings-field__label">可执行文件路径</span>
                <input
                  aria-label="本机 CLI 可执行文件路径"
                  autoComplete="off"
                  className="settings-field__control"
                  data-testid="runtime-slots-executable"
                  disabled={busy}
                  onChange={(event) => setExecutablePath(event.target.value)}
                  placeholder="绝对路径，例如 C:\\tools\\codex.exe"
                  spellCheck={false}
                  type="text"
                  value={executablePath}
                />
              </label>
              <label className="settings-field">
                <span className="settings-field__label">
                  调用参数（空格分隔，可空）
                </span>
                <input
                  aria-label="本机 CLI 调用参数"
                  autoComplete="off"
                  className="settings-field__control"
                  data-testid="runtime-slots-launch-args"
                  disabled={busy}
                  onChange={(event) => setLaunchArgsText(event.target.value)}
                  spellCheck={false}
                  type="text"
                  value={launchArgsText}
                />
              </label>
              <label className="settings-field">
                <span className="settings-field__label">
                  账号状态参数（空格分隔，必填）
                </span>
                <input
                  aria-label="本机 CLI 账号状态参数"
                  autoComplete="off"
                  className="settings-field__control"
                  data-testid="runtime-slots-status-args"
                  disabled={busy}
                  onChange={(event) => setStatusArgsText(event.target.value)}
                  placeholder="例如 login status"
                  spellCheck={false}
                  type="text"
                  value={statusArgsText}
                />
              </label>
            </>
          ) : null}

          {formMode === "create" && formStep === "connect" && createAuthKind === "grok_login" ? (
            <p
              className="runtime-slots__hint"
              data-testid="runtime-slots-login-hint"
              role="note"
            >
              「已登录的 Grok」不会弹出浏览器。Pillow 使用本机默认 Grok CLI
              登录 profile，不存储该登录。请先在终端运行{" "}
              <code>grok login</code>，完成后再创建槽位。
            </p>
          ) : null}

          {formMode === "create" && formStep === "connect" && createAuthKind === "oauth" ? (
            <div
              className="runtime-slots-form__oauth"
              data-testid="runtime-slots-oauth-form"
            >
              <p className="runtime-slots__hint" role="note">
                {OAUTH_LOGIN_FORM_HINT}
              </p>
              {oauthCatalogBusy ? (
                <p
                  className="runtime-slots__hint"
                  data-testid="runtime-slots-oauth-catalog-loading"
                  role="status"
                >
                  {OAUTH_CATALOG_LOAD_BUSY}
                </p>
              ) : null}
              {oauthCatalogError ? (
                <p
                  className="runtime-slots__error"
                  data-testid="runtime-slots-oauth-catalog-error"
                  role="alert"
                >
                  {oauthCatalogError}
                </p>
              ) : null}
              <span hidden data-testid="runtime-slots-oauth-provider">
                {oauthProviderId}
              </span>
              <span hidden data-testid="runtime-slots-oauth-provider-ids">
                {oauthProviderId}
              </span>

              {selectedOauthProvider ? (
                <p
                  className="runtime-slots__hint"
                  data-testid="runtime-slots-oauth-method"
                  role="note"
                >
                  登录方式：
                  {selectedOauthProvider.loginMethod === "pkce"
                    ? "Authorization Code + PKCE"
                    : "Device Code"}
                </p>
              ) : null}

              {oauthRequiredFields.map((field) => (
                <label className="settings-field" key={field.fieldId}>
                  <span className="settings-field__label">
                    {field.label}
                    <span className="runtime-slots-badge">必填</span>
                  </span>
                  <input
                    aria-label={field.label}
                    autoComplete="off"
                    className="settings-field__control"
                    data-testid={`runtime-slots-oauth-field-${field.fieldId}`}
                    disabled={busy || oauthLoginBusy}
                    onChange={(event) =>
                      setOauthField(field.fieldId, event.target.value)}
                    spellCheck={false}
                    type="text"
                    value={oauthDraftFields[field.fieldId]}
                  />
                  <span
                    className="runtime-slots__hint"
                    data-testid={`runtime-slots-oauth-field-reason-${field.fieldId}`}
                    role="note"
                  >
                    {field.reason}
                  </span>
                </label>
              ))}

              {/*
                Host create_slot oauth still requires a call endpoint.
                These are slot call settings, not OAuth authorize fields.
              */}
              <label className="settings-field">
                <span className="settings-field__label">
                  调用端点 base_url
                </span>
                <input
                  aria-label="调用端点 base_url"
                  autoComplete="off"
                  className="settings-field__control"
                  data-testid="runtime-slots-base-url"
                  disabled={busy || oauthLoginBusy}
                  onChange={(event) => setBaseUrl(event.target.value)}
                  placeholder="https://…（授权完成后用此地址发请求）"
                  spellCheck={false}
                  type="text"
                  value={baseUrl}
                />
              </label>
              <label className="settings-field">
                <span className="settings-field__label">调用模型 id</span>
                <input
                  aria-label="调用模型 id"
                  autoComplete="off"
                  className="settings-field__control"
                  data-testid="runtime-slots-model-id"
                  disabled={busy || oauthLoginBusy}
                  onChange={(event) => setModelId(event.target.value)}
                  placeholder="例如 gpt-4o"
                  spellCheck={false}
                  type="text"
                  value={modelId}
                />
              </label>

              {oauthLoginBusy ? (
                <p
                  className="runtime-slots__notice"
                  data-testid="runtime-slots-oauth-in-progress"
                  role="status"
                >
                  {OAUTH_LOGIN_IN_PROGRESS}
                </p>
              ) : null}
              {oauthLoginBusy && !oauthBrowserOpened ? (
                <p
                  className="runtime-slots__hint"
                  data-testid="runtime-slots-oauth-browser-failed-hint"
                  role="status"
                >
                  {OAUTH_BROWSER_OPEN_FAILED_MESSAGE}
                </p>
              ) : null}
              {oauthUserCode ? (
                <p
                  className="runtime-slots__hint"
                  data-testid="runtime-slots-oauth-user-code"
                  role="status"
                >
                  设备码：<code>{oauthUserCode}</code>
                </p>
              ) : null}
              {oauthAuthorizeUrl ? (
                <label
                  className="settings-field"
                  data-testid="runtime-slots-oauth-authorize-url-field"
                >
                  <span className="settings-field__label">
                    授权链接（可复制，手动打开）
                  </span>
                  <input
                    aria-label="OAuth 授权链接"
                    className="settings-field__control"
                    data-testid="runtime-slots-oauth-authorize-url"
                    readOnly
                    spellCheck={false}
                    type="text"
                    value={oauthAuthorizeUrl}
                  />
                </label>
              ) : null}
            </div>
          ) : null}

          {formMode === "edit" || formStep === "connect" ? (
            <>
              <div
                className="runtime-slots-form__kind"
                role="group"
                aria-label="角色"
              >
                <span className="settings-field__label">角色</span>
                {SLOT_ROLE_OPTIONS.map((option) => {
                  const disabled =
                    option.id === "controller" && !controllerAllowedForCreate
                    && formMode === "create";
                  return (
                    <button
                      aria-pressed={createRole === option.id}
                      data-testid={`runtime-slots-role-${option.id}`}
                      disabled={disabled || busy}
                      key={option.id}
                      onClick={() => setCreateRole(option.id)}
                      title={
                        disabled
                          ? CONTROLLER_REQUIRES_GOVERNED_AUTH_HINT
                          : option.description
                      }
                      type="button"
                    >
                      {option.label}
                    </button>
                  );
                })}
              </div>
              {formMode === "create" && !controllerAllowedForCreate ? (
                <p
                  className="runtime-slots__hint"
                  data-testid="runtime-slots-controller-cli-disabled"
                  role="note"
                >
                  {CONTROLLER_REQUIRES_GOVERNED_AUTH_HINT}
                </p>
              ) : null}
              {createRole === "custom" ? (
                <label className="settings-field">
                  <span className="settings-field__label">自定义角色名</span>
                  <input
                    aria-label="自定义角色名"
                    autoComplete="off"
                    className="settings-field__control"
                    data-testid="runtime-slots-role-custom-name"
                    disabled={busy}
                    onChange={(event) => setRoleCustomName(event.target.value)}
                    placeholder="例如：写码、审阅"
                    spellCheck={false}
                    type="text"
                    value={roleCustomName}
                  />
                </label>
              ) : null}
            </>
          ) : null}

          {/* Hidden marker for tests asserting ungoverned copy constant. */}
          <span hidden data-testid="runtime-slots-ungoverned-copy">
            {EXTERNAL_CLI_UNGOVERNED_BANNER}
          </span>

          <div className="settings-actions modal-actions">
            <button
              data-testid="runtime-slots-form-cancel"
              disabled={busy}
              onClick={closeForm}
              type="button"
            >
              取消
            </button>
            {formMode === "edit" || formStep === "connect" ? (
              <button
                data-testid="runtime-slots-form-save"
                disabled={busy || oauthLoginBusy}
                type="submit"
              >
                {oauthLoginBusy
                  ? "登录中…"
                  : busy
                  ? "保存中…"
                  : createAuthKind === "oauth" && formMode === "create"
                  ? "开始 OAuth 登录"
                  : "保存"}
              </button>
            ) : null}
          </div>
        </form>
      )}

      {error ? (
        <p
          className="runtime-slots__error"
          data-testid="runtime-slots-error"
          role="alert"
        >
          {error}
        </p>
      ) : null}
      {notice ? (
        <p
          className="runtime-slots__notice"
          data-testid="runtime-slots-notice"
          role="status"
        >
          {notice}
        </p>
      ) : null}
    </section>
  );
}
