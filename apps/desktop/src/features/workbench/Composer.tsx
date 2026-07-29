import {
  IconFolder,
  IconFolderPlus,
  IconSend,
} from "../../components/icons";

type ComposerProps = {
  disabled?: boolean;
  projectName?: string | null;
  busy?: boolean;
  onOpenProject?: () => void;
  onNewProject?: () => void;
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
  onOpenProject,
  onNewProject,
}: ComposerProps) {
  return (
    <div className="composer-wrap composer-wrap--float">
      <form
        aria-label="工作请求"
        className="composer"
        data-testid="composer"
        onSubmit={(event) => event.preventDefault()}
      >
        <label className="sr-only" htmlFor="work-request">
          工作请求
        </label>
        <textarea
          disabled={disabled}
          id="work-request"
          placeholder={
            disabled
              ? "打开或新建项目后开始描述工作"
              : "描述要完成的工作"
          }
          rows={2}
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
          <button
            aria-label="发送"
            className="icon-btn icon-btn--primary"
            data-testid="composer-send"
            disabled
            title="发送（代理运行时尚未接入）"
            type="submit"
          >
            <IconSend size={16} />
          </button>
        </div>
      </form>
    </div>
  );
}
