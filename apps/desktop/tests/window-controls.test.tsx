import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import {
  WindowControls,
  type DesktopWindowApi,
} from "../src/components/WindowControls";

function fixtureWindow() {
  let maximized = false;
  let resized: (() => void) | undefined;
  const unlisten = vi.fn();
  const window: DesktopWindowApi = {
    minimize: vi.fn().mockResolvedValue(undefined),
    toggleMaximize: vi.fn().mockImplementation(async () => {
      maximized = !maximized;
    }),
    close: vi.fn().mockResolvedValue(undefined),
    isMaximized: vi.fn().mockImplementation(async () => maximized),
    onResized: vi.fn().mockImplementation(async (handler: () => void) => {
      resized = handler;
      return unlisten;
    }),
  };

  return {
    window,
    unlisten,
    setMaximized(value: boolean) {
      maximized = value;
    },
    emitResize() {
      resized?.();
    },
  };
}

describe("WindowControls", () => {
  it("calls native actions and tracks external maximize changes", async () => {
    const user = userEvent.setup();
    const fixture = fixtureWindow();
    const { unmount } = render(
      <WindowControls getWindow={async () => fixture.window} />,
    );

    await waitFor(() => expect(fixture.window.onResized).toHaveBeenCalledOnce());
    const maximize = screen.getByRole("button", { name: "最大化" });
    const maximizeGlyph = maximize.textContent;

    await user.click(screen.getByRole("button", { name: "最小化" }));
    await user.click(maximize);

    expect(fixture.window.minimize).toHaveBeenCalledOnce();
    expect(fixture.window.toggleMaximize).toHaveBeenCalledOnce();
    await waitFor(() => {
      const restore = screen.getByRole("button", { name: "还原" });
      expect(restore.textContent).not.toBe(maximizeGlyph);
    });

    fixture.setMaximized(false);
    await act(async () => fixture.emitResize());
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "最大化" })).toBeVisible();
    });

    await user.click(screen.getByRole("button", { name: "关闭" }));
    expect(fixture.window.close).toHaveBeenCalledOnce();

    unmount();
    expect(fixture.unlisten).toHaveBeenCalledOnce();
  });

  it("reports native failures instead of silently swallowing them", async () => {
    const user = userEvent.setup();
    const fixture = fixtureWindow();
    const failure = new Error("native minimize failed");
    vi.mocked(fixture.window.minimize).mockRejectedValue(failure);
    const report = vi.spyOn(console, "error").mockImplementation(() => undefined);
    render(<WindowControls getWindow={async () => fixture.window} />);

    await waitFor(() => expect(fixture.window.onResized).toHaveBeenCalledOnce());
    await user.click(screen.getByRole("button", { name: "最小化" }));

    await waitFor(() => {
      expect(report).toHaveBeenCalledWith(
        "[Pillow] Window action failed: minimize",
        failure,
      );
    });
    report.mockRestore();
  });
});
