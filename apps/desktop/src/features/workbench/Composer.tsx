/**
 * Simplified composer shell adapted from Grok App ComposerEditor layout.
 * Upstream: src/components/ComposerEditor.tsx + composer styles in app.css at
 * cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.
 * No model/provider/account/quota controls in this prototype.
 */
export function Composer({ disabled }: { disabled?: boolean }) {
  return (
    <div className="composer-wrap">
      <form
        aria-label="工作请求"
        className="composer"
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
          rows={3}
        />
        <div className="composer__footer">
          <span>运行时将在创建对话后选择</span>
          <button disabled type="submit">
            发送
          </button>
        </div>
      </form>
    </div>
  );
}
