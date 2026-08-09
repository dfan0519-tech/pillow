/**
 * First-run onboarding surface for strangers with an empty project list.
 *
 * Neutral status guidance (not an error). Primary next step is open project;
 * Grok readiness is educational copy aligned with ADR-0003 (CLI owns login).
 * Dismiss is durable via the settings layer — this component only invokes
 * callbacks; it never opens a second store.
 */
import {
  FIRSTRUN_CLOSE_LABEL,
  FIRSTRUN_DISMISS_LABEL,
  FIRSTRUN_GROK_BODY,
  FIRSTRUN_OPEN_PROJECT_LABEL,
  FIRSTRUN_OPEN_SETTINGS_LABEL,
  FIRSTRUN_PROJECT_BODY,
  FIRSTRUN_TITLE,
} from "./onboardingCopy";
import "../../styles/onboarding.css";

export type FirstRunOnboardingProps = {
  onOpenProject: () => void;
  onOpenSettings: () => void;
  onDismiss: () => void;
  busy?: boolean;
};

export function FirstRunOnboarding({
  onOpenProject,
  onOpenSettings,
  onDismiss,
  busy = false,
}: FirstRunOnboardingProps) {
  return (
    <section
      aria-labelledby="firstrun-onboarding-title"
      className="firstrun-onboarding"
      data-testid="firstrun-onboarding"
      role="region"
    >
      <div className="firstrun-onboarding__body">
        <strong id="firstrun-onboarding-title">{FIRSTRUN_TITLE}</strong>
        <p data-testid="firstrun-project-body">{FIRSTRUN_PROJECT_BODY}</p>
        <p data-testid="firstrun-grok-body">{FIRSTRUN_GROK_BODY}</p>
      </div>
      <div className="firstrun-onboarding__actions">
        <button
          className="firstrun-onboarding__cta"
          data-testid="firstrun-open-project"
          disabled={busy}
          onClick={onOpenProject}
          type="button"
        >
          {FIRSTRUN_OPEN_PROJECT_LABEL}
        </button>
        <button
          data-testid="firstrun-open-settings"
          disabled={busy}
          onClick={onOpenSettings}
          type="button"
        >
          {FIRSTRUN_OPEN_SETTINGS_LABEL}
        </button>
        <button
          data-testid="firstrun-dismiss"
          disabled={busy}
          onClick={onDismiss}
          type="button"
        >
          {FIRSTRUN_DISMISS_LABEL}
        </button>
        <button
          aria-label={FIRSTRUN_CLOSE_LABEL}
          className="firstrun-onboarding__close"
          data-testid="firstrun-close"
          disabled={busy}
          onClick={onDismiss}
          type="button"
        >
          ×
        </button>
      </div>
    </section>
  );
}
