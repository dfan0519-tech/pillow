import { useState, type FormEvent, type KeyboardEvent } from "react";
import {
  IconClose,
  IconFolder,
  IconFolderPlus,
  IconSend,
} from "../../components/icons";

type ComposerProps = {
  disabled?: boolean;
  projectName?: string | null;
  busy?: boolean;
  sending?: boolean;
  canCancel?: boolean;
  onOpenProject?: () => void;
  onNewProject?: () => void;
  onSend?: (content: string) => void;
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
  busy = false,
  sending = false,
  canCancel = false,
  onOpenProject,
  onNewProject,
  onSend,
  onCancelRun,
}: ComposerProps) {
  const [draft, setDraft] = useState("");
  const canSend =
    !disabled && !busy && !sending && !canCancel && draft.trim().length > 0;

  function submit() {
    const content = draft.trim();
    if (!content || !canSend || !onSend) return;
    onSend(content);
    setDraft("");
  }

  function handleSubmit(event: FormEvent) {
    event.preventDefault();
    submit();
  }

  function handleKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      submit();
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
          onKeyDown={handleKeyDown}
          placeholder={
            disabled
              ? "打开或新建项目后开始描述工作"
              : canCancel
                ? "运行中…可取消当前回合"
                : "描述要完成的工作"
          }
          rows={2}
          value={draft}
        />
        <div className="composer__footer">
          <div className="composer__context">
            {projectName ? (
              <span className="composer-chip" title={projectName}>
                <IconFolder size={14} />
                <span>{projectName}</span>
              </span>
            ) : (
              <>
                <button
                  className="composer-chip composer-chip--action"
                  disabled={busy}
                  onClick={onOpenProject}
                  type="button"
                >
                  <IconFolder size={14} />
                  <span>打开项目</span>
                </button>
                <button
                  className="composer-chip composer-chip--action"
                  disabled={busy}
                  onClick={onNewProject}
                  type="button"
                >
                  <IconFolderPlus size={14} />
                  <span>新建项目</span>
                </button>
              </>
            )}
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
