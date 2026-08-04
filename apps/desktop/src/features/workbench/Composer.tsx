import { useState, type FormEvent, type KeyboardEvent } from "react";
import {
  IconClose,
  IconFolder,
  IconLock,
  IconSend,
  IconTerminal,
} from "../../components/icons";

type ComposerProps = {
  disabled?: boolean;
  projectName?: string | null;
  runtimeLabel?: string;
  busy?: boolean;
  sending?: boolean;
  canCancel?: boolean;
  sendError?: string | null;
  /** Non-destructive notice after an accepted send (e.g. timeline refresh failed). */
  refreshWarning?: string | null;
  onOpenProject?: () => void;
  /** Resolves on success; rejects so the composer can restore the draft. */
  onSend?: (content: string) => void | Promise<void>;
  onCancelRun?: () => void;
};

/**
 * Floating composer shell adapted from Grok App composer styles.
 * Upstream: composer card + icon-btn--primary in src/styles/app.css and
 * ComposerEditor layout at cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.
 * No model/provider/account/quota controls in this prototype.
 */
export function Composer({
  disabled = false,
  projectName = null,
  runtimeLabel = "本地",
  busy = false,
  sending = false,
  canCancel = false,
  sendError = null,
  refreshWarning = null,
  onOpenProject,
  onSend,
  onCancelRun,
}: ComposerProps) {
  const [draft, setDraft] = useState("");
  const [composing, setComposing] = useState(false);
  const canSend =
    !disabled && !busy && !sending && !canCancel && draft.trim().length > 0;

  async function submit() {
    const content = draft.trim();
    if (!content || !canSend || !onSend) return;
    setDraft("");
    try {
      await onSend(content);
    } catch {
      // Preserve / recover draft after a failed send.
      setDraft(content);
    }
  }

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    void submit();
  }

  function handleKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    // Ignore Enter while IME composition is active (e.g. Chinese/Japanese).
    if (event.nativeEvent.isComposing || event.keyCode === 229 || composing) {
      return;
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void submit();
    }
  }

  return (
    <div className="composer-wrap composer-wrap--float">
      <form
        aria-label="对话输入"
        className="composer"
        data-testid="composer"
        onSubmit={handleSubmit}
      >
        <label className="sr-only" htmlFor="work-request">
          工作请求
        </label>
        <textarea
          disabled={disabled || canCancel}
          id="work-request"
          onChange={(event) => setDraft(event.target.value)}
          onCompositionEnd={() => setComposing(false)}
          onCompositionStart={() => setComposing(true)}
          onKeyDown={handleKeyDown}
          placeholder={
            disabled
              ? "打开现有项目后开始描述工作"
              : canCancel
                ? "运行中…可取消当前回合"
                : "描述要完成的工作"
          }
          rows={2}
          value={draft}
        />
        {sendError ? (
          <p className="composer__error" data-testid="composer-send-error" role="alert">
            {sendError}
          </p>
        ) : null}
        {refreshWarning && !sendError ? (
          <p
            className="composer__warning"
            data-testid="composer-refresh-warning"
            role="status"
          >
            {refreshWarning}
          </p>
        ) : null}
        <div className="composer__footer">
          <div aria-label="执行上下文" className="composer__context">
            {projectName ? (
              <span
                aria-label={`项目：${projectName}`}
                className="composer-context__item"
                title={projectName}
              >
                <IconFolder size={14} />
                <span className="composer-context__label">项目</span>
                <span className="composer-context__value">{projectName}</span>
              </span>
            ) : (
              <button
                className="composer-context__item composer-context__action"
                disabled={busy}
                onClick={onOpenProject}
                type="button"
              >
                <IconFolder size={14} />
                <span>打开项目</span>
              </button>
            )}
            <span
              aria-label={`运行时：${runtimeLabel}`}
              className="composer-context__item"
            >
              <IconTerminal size={14} />
              <span className="composer-context__label">运行时</span>
              <span className="composer-context__value">{runtimeLabel}</span>
            </span>
            <span
              aria-label="权限：按需批准"
              className="composer-context__item"
            >
              <IconLock size={14} />
              <span className="composer-context__label">权限</span>
              <span className="composer-context__value">按需批准</span>
            </span>
          </div>
          <div className="composer__actions">
            {canCancel ? (
              <button
                aria-label="取消运行"
                className="icon-btn"
                data-testid="composer-cancel"
                onClick={onCancelRun}
                title="取消当前运行"
                type="button"
              >
                <IconClose size={16} />
              </button>
            ) : null}
            <button
              aria-label="发送"
              className="icon-btn icon-btn--primary"
              data-testid="composer-send"
              disabled={!canSend}
              title={canSend ? "发送" : "发送"}
              type="submit"
            >
              <IconSend size={16} />
            </button>
          </div>
        </div>
      </form>
    </div>
  );
}
