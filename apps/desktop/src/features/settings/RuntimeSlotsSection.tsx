/**
 * Project-scoped runtime slots: unified create (name → auth → role),
 * role reassignment, and API-key edit (keep-on-blank).
 * Secrets never enter localStorage / sessionStorage / URL; drafts clear on save.
 * Host errors are never console-logged (may carry secret-shaped detail).
 */

import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { RuntimeSlotModelPicker } from "./RuntimeSlotModelPicker";
import {
  MockRuntimeSlotsGateway,
  TauriRuntimeSlotsGateway,
  classifyExternalCliStatus,
  mapGrokProbeFailureMessage,
  mapRuntimeSlotErrorMessage,
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
  SECRET_CONFIGURED_LABEL,
  SECRET_NOT_CONFIGURED_LABEL,
  SLOT_AUTH_METHODS,
  SLOT_ROLE_OPTIONS,
  type RuntimeSlotModelSwitchView,
  type RuntimeSlotRole,
  type RuntimeSlotView,
  type RuntimeSlotsGateway,
  type SlotAuthKind,
  RuntimeSlotsGatewayError,
} from "./runtimeSlotsTypes";
import "../../styles/runtime-slots.css";

export type RuntimeSlotsSectionProps = {
  projectId: string | null;
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
    return mapRuntimeSlotErrorMessage(cause.code);
  }
  if (cause instanceof Error && cause.message.trim()) {
    return cause.message;
  }
  return "保存槽位失败。";
}

export function RuntimeSlotsSection({
  projectId,
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
  /**
   * Last successfully started governed slot in this panel session.
   * Host list DTO has no active flag — session-local only so Start stays
   * obvious for slots that have never been started here.
   */
  const [startedSlotId, setStartedSlotId] = useState<string | null>(null);

  const [formMode, setFormMode] = useState<FormMode>("closed");
  const [editingSlotId, setEditingSlotId] = useState<string | null>(null);
  /** Unified create auth — data-driven from SLOT_AUTH_METHODS. */
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

  /** Inline role editor for an existing list row. */
  const [roleEditSlotId, setRoleEditSlotId] = useState<string | null>(null);
  const [roleEditRole, setRoleEditRole] = useState<RuntimeSlotRole>("custom");
  const [roleEditCustomName, setRoleEditCustomName] = useState("");

  const hasController = useMemo(
    () => slots.some((slot) => slot.role === "controller"),
    [slots],
  );
  const hasLibrarian = useMemo(
    () => slots.some((slot) => slot.role === "librarian"),
    [slots],
  );
  const controllerAllowedForCreate = authAllowsController(createAuthKind);

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
    setStartedSlotId(null);
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
    setCreateAuthKind("api_key");
    setCreateRole("controller");
    setRoleCustomName("");
  }

  function openCreate(): void {
    resetFormDrafts();
    // Suggest controller only when the project has none yet.
    setCreateRole(hasController ? "custom" : "controller");
    setFormMode("create");
    setRoleEditSlotId(null);
    setNotice(null);
    setError(null);
    setProbe(null);
  }

  function openEdit(slot: RuntimeSlotView): void {
    if (slot.kind !== "governed_grok" || slot.authMode !== "api_key") {
      return;
    }
    setFormMode("edit");
    setEditingSlotId(slot.slotId);
    setCreateAuthKind("api_key");
    setCreateRole(slot.role);
    setRoleCustomName(slot.roleCustomName ?? "");
    setDisplayName(slot.displayName);
    setBaseUrl(slot.baseUrl ?? "");
    setModelId(slot.modelId ?? "");
    // Leave secret field blank = keep stored value.
    setApiKeyDraft("");
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
    setCreateAuthKind(kind);
    if (!authAllowsController(kind) && createRole === "controller") {
      setCreateRole("custom");
    }
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
          setNotice("已创建槽位（使用本机默认 Grok 登录 profile）。");
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
      setError(mapSaveError(cause));
    } finally {
      setBusy(false);
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
        setStartedSlotId(slot.slotId);
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
          所有槽位平等：新建时填写名称、选择认证方式、分配角色。
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
              className="runtime-slots-list__item"
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
                    {slot.kind === "governed_grok"
                      ? slot.authMode === "login"
                        ? "Grok · 登录"
                        : "Grok · API key"
                      : "本机 CLI"}
                  </span>
                  <span
                    className="runtime-slots-badge"
                    data-testid={`runtime-slot-role-${slot.slotId}`}
                  >
                    {roleLabel(slot)}
                  </span>
                  {slot.kind === "governed_grok"
                  && slot.governance === "full" ? (
                    startedSlotId === slot.slotId ? (
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
                        startedSlotId === slot.slotId
                          ? "runtime-slots-start"
                          : "runtime-slots-start runtime-slots-start--primary"
                      }
                      data-testid={`runtime-slot-probe-${slot.slotId}`}
                      disabled={busy}
                      onClick={() => void handleProbe(slot)}
                      type="button"
                    >
                      {startedSlotId === slot.slotId ? "重新启动" : "启动"}
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
          onSubmit={(event) => void handleSave(event)}
        >
          <p className="runtime-slots-form__title">
            {formMode === "edit" ? "编辑 API-key 槽位" : "新建槽位"}
          </p>

          <label className="settings-field">
            <span className="settings-field__label">1. 名称</span>
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

          {formMode === "create" ? (
            <>
              <div
                className="runtime-slots-form__kind"
                role="group"
                aria-label="2. 认证方式"
              >
                <span className="settings-field__label">2. 认证方式</span>
                {SLOT_AUTH_METHODS.map((method) => (
                  <button
                    aria-pressed={createAuthKind === method.id}
                    data-testid={`runtime-slots-auth-${method.id}`}
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
                {SLOT_AUTH_METHODS.find((m) => m.id === createAuthKind)
                  ?.description}
              </p>
            </>
          ) : null}

          {formMode === "create" && createAuthKind === "local_cli" ? (
            <p
              className="runtime-slots-ungoverned"
              data-testid="runtime-slots-create-ungoverned-warning"
              role="note"
            >
              <strong>{EXTERNAL_CLI_UNGOVERNED_HEADING}</strong>
              。{EXTERNAL_CLI_UNGOVERNED_BODY}
            </p>
          ) : null}

          {(formMode === "edit"
            || (formMode === "create" && createAuthKind === "api_key")) ? (
            <>
              <label className="settings-field">
                <span className="settings-field__label">端点 base_url</span>
                <input
                  aria-label="端点 base_url"
                  autoComplete="off"
                  className="settings-field__control"
                  data-testid="runtime-slots-base-url"
                  disabled={busy}
                  onChange={(event) => setBaseUrl(event.target.value)}
                  placeholder="https://…"
                  spellCheck={false}
                  type="text"
                  value={baseUrl}
                />
              </label>
              <label className="settings-field">
                <span className="settings-field__label">
                  模型 id
                  <span className="runtime-slots-badge">
                    创建/编辑表单手输；已有槽位可在列表中快速切换
                  </span>
                </span>
                <input
                  aria-label="模型 id"
                  autoComplete="off"
                  className="settings-field__control"
                  data-testid="runtime-slots-model-id"
                  disabled={busy}
                  onChange={(event) => setModelId(event.target.value)}
                  spellCheck={false}
                  type="text"
                  value={modelId}
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
            </>
          ) : null}

          {formMode === "create" && createAuthKind === "local_cli" ? (
            <>
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

          {formMode === "create" && createAuthKind === "grok_login" ? (
            <p
              className="runtime-slots__hint"
              data-testid="runtime-slots-login-hint"
              role="note"
            >
              使用本机默认 Grok 登录 profile。登录由 Grok CLI 管理，Pillow
              不存储该登录。
            </p>
          ) : null}

          <div
            className="runtime-slots-form__kind"
            role="group"
            aria-label="3. 角色"
          >
            <span className="settings-field__label">3. 角色</span>
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
            <button
              data-testid="runtime-slots-form-save"
              disabled={busy}
              type="submit"
            >
              {busy ? "保存中…" : "保存"}
            </button>
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
