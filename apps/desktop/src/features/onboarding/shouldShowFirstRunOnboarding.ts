import type { RuntimeStartupView } from "../settings/settingsTypes";

export type FirstRunOnboardingInput = {
  /** null while the settings read is in flight — do not flash then hide. */
  dismissed: boolean | null;
  /** Registered projects from the shell list (empty = stranger shell). */
  projectsCount: number;
  /** null while startup status is loading. */
  runtimeStartup: RuntimeStartupView | null;
  /** Project list has settled (success path). */
  shellReady: boolean;
};

/**
 * True when first-run guidance should paint.
 *
 * Distinguishes **never configured** (show) from **misconfigured** (degraded
 * banner only) and from **already set up** (negative control — never nag).
 */
export function shouldShowFirstRunOnboarding(
  input: FirstRunOnboardingInput,
): boolean {
  if (!input.shellReady) return false;
  // Still loading dismiss flag — wait so we do not flash for configured users.
  if (input.dismissed === null) return false;
  if (input.dismissed) return false;
  if (input.runtimeStartup === null) return false;

  // Misconfigured paths / release Fake: existing degraded banner owns the UX.
  // First-run guidance must not present that state as "welcome".
  if (
    !input.runtimeStartup.conversationAvailable
    && input.runtimeStartup.degradedCode
  ) {
    return false;
  }

  // Negative control: any registered project means the user already configured
  // a workspace. Do not re-show even if dismiss was never written.
  if (input.projectsCount > 0) return false;

  return true;
}

/**
 * True when startup is the degraded (misconfigured) path — not first-run.
 */
export function isRuntimeDegraded(
  runtimeStartup: RuntimeStartupView | null,
): boolean {
  return Boolean(
    runtimeStartup
    && !runtimeStartup.conversationAvailable
    && runtimeStartup.degradedCode,
  );
}
