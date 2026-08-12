import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { LibrarianMonitorSection } from "../src/features/settings/LibrarianMonitorSection";
import type {
  LibrarianAuditGateway,
  LibrarianAuditReport,
} from "../src/features/settings/librarianAuditTypes";
import { MockLibrarianAuditGateway } from "../src/features/settings/librarianAuditGateway";
import { MockRuntimeSlotsGateway } from "../src/features/settings/runtimeSlotsGateway";
import type { RuntimeSlotView } from "../src/features/settings/runtimeSlotsTypes";
import { MockSettingsGateway } from "../src/features/settings/settingsGateway";
import {
  SettingsPanel,
  type SettingsPanelProps,
} from "../src/features/settings/SettingsPanel";

const PASS_REPORT: LibrarianAuditReport = {
  projectId: "project-1",
  outcome: "pass",
  checkedAt: "2026-08-12T03:00:00.000Z",
  checks: [
    {
      id: "inflight_handoff_consistency",
      status: "pass",
      summary: "交接中的在途计数与台账一致。",
      findings: [],
    },
    {
      id: "pending_acceptance_visibility",
      status: "pass",
      summary: "所有待验收任务都已在交接中提及。",
      findings: [],
    },
    {
      id: "handoff_ledger_references",
      status: "pass",
      summary: "交接提及的任务均可在台账中找到。",
      findings: [],
    },
    {
      id: "inflight_inventory",
      status: "pass",
      summary: "在途清单已完整枚举，共 2 项。",
      findings: [],
    },
  ],
};

const FAIL_REPORT: LibrarianAuditReport = {
  ...PASS_REPORT,
  outcome: "fail",
  checks: PASS_REPORT.checks.map((check) => {
    if (check.id === "inflight_handoff_consistency") {
      return {
        ...check,
        status: "fail" as const,
        summary: "交接声称 0 项在途，但台账记录 2 项。",
        findings: [
          {
            code: "inflight_count_mismatch",
            summary: "交接计数 0 与台账计数 2 不一致。",
          },
          {
            code: "inflight_item_unmentioned",
            summary: "任务 build-monitor 未在交接中提及。",
          },
        ],
      };
    }
    return check;
  }),
};

function auditGateway(report: LibrarianAuditReport): LibrarianAuditGateway {
  return new MockLibrarianAuditGateway([report]);
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, reject, resolve };
}

function slot(
  role: RuntimeSlotView["role"],
  displayName: string,
): RuntimeSlotView {
  return {
    slotId: `${role}-slot`,
    projectId: "project-1",
    displayName,
    kind: "governed_grok",
    governance: "full",
    authMode: "login",
    baseUrl: null,
    modelId: null,
    secretRef: null,
    secretConfigured: false,
    executablePath: null,
    launchArgs: null,
    statusArgs: null,
    role,
    roleCustomName: role === "custom" ? "写码" : null,
    createdAt: "2026-08-12T02:00:00.000Z",
    updatedAt: "2026-08-12T02:00:00.000Z",
    accountLabel: null,
    accessExpiresAt: null,
  };
}

function reportForProject(
  projectId: string,
  firstSummary: string,
): LibrarianAuditReport {
  return {
    ...PASS_REPORT,
    projectId,
    checks: PASS_REPORT.checks.map((check, index) =>
      index === 0 ? { ...check, summary: firstSummary } : { ...check },
    ),
  };
}

describe("Librarian monitoring surface", () => {
  it("runs the built-in project health surface inside settings without inventing FAIL", async () => {
    const runLibrarianAudit = vi.fn().mockResolvedValue(PASS_REPORT);
    const props = {
      gateway: new MockSettingsGateway(),
      projectId: "project-1",
      onClose: () => undefined,
      librarianAuditGateway: { runLibrarianAudit },
    } as SettingsPanelProps;

    render(<SettingsPanel {...props} />);

    // Diagnostics category hosts the librarian monitor surface.
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("settings-nav-diagnostics"));
    const section = await screen.findByTestId("librarian-monitor-section");
    expect(await within(section).findAllByText("PASS")).toHaveLength(4);
    expect(within(section).queryByText("FAIL")).toBeNull();
    expect(runLibrarianAudit).toHaveBeenCalledTimes(1);
    expect(runLibrarianAudit).toHaveBeenCalledWith("project-1");
  });

  it("renders all four PASS checks with evidence and no failure affordance", async () => {
    const { container } = render(
      <LibrarianMonitorSection
        auditGateway={auditGateway(PASS_REPORT)}
        projectId="project-1"
      />,
    );

    const section = await screen.findByTestId("librarian-monitor-section");
    const expectedRows = [
      [
        "inflight_handoff_consistency",
        "在途计数与交接一致",
        "交接中的在途计数与台账一致。",
      ],
      [
        "pending_acceptance_visibility",
        "待验收任务可见",
        "所有待验收任务都已在交接中提及。",
      ],
      [
        "handoff_ledger_references",
        "交接任务均有台账记录",
        "交接提及的任务均可在台账中找到。",
      ],
      [
        "inflight_inventory",
        "在途清单可枚举",
        "在途清单已完整枚举，共 2 项。",
      ],
    ] as const;
    for (const [id, label, summary] of expectedRows) {
      const row = within(section).getByTestId(`librarian-check-${id}`);
      expect(within(row).getByText(label)).toBeVisible();
      expect(within(row).getByText("PASS")).toBeVisible();
      expect(within(row).getByText(summary)).toBeVisible();
      expect(within(row).queryByText("FAIL")).toBeNull();
      expect(row.querySelector("details")).toBeNull();
    }
    expect(within(section).queryByText("FAIL")).toBeNull();
    expect(container.querySelector("details")).toBeNull();
  });

  it("renders FAIL evidence as expandable findings without weakening PASS rows", async () => {
    const user = userEvent.setup();
    render(
      <LibrarianMonitorSection
        auditGateway={auditGateway(FAIL_REPORT)}
        projectId="project-1"
      />,
    );

    const failedRow = await screen.findByTestId(
      "librarian-check-inflight_handoff_consistency",
    );
    const passedRow = screen.getByTestId(
      "librarian-check-pending_acceptance_visibility",
    );
    expect(within(failedRow).getByText("FAIL")).toBeVisible();
    expect(within(failedRow).queryByText("PASS")).toBeNull();
    expect(
      within(failedRow).getByText("交接声称 0 项在途，但台账记录 2 项。"),
    ).toBeVisible();
    expect(within(passedRow).getByText("PASS")).toBeVisible();
    expect(within(passedRow).queryByText("FAIL")).toBeNull();
    expect(within(passedRow).queryByText(/查看 .*问题/u)).toBeNull();

    await user.click(within(failedRow).getByText("查看 2 条问题"));
    expect(
      within(failedRow).getByText("交接计数 0 与台账计数 2 不一致。"),
    ).toBeVisible();
    expect(
      within(failedRow).getByText("任务 build-monitor 未在交接中提及。"),
    ).toBeVisible();
    expect(within(failedRow).queryByText("不存在的问题")).toBeNull();
  });

  it("clears the previous report while a manual rerun is in progress and then refreshes", async () => {
    const user = userEvent.setup();
    const rerun = deferred<LibrarianAuditReport>();
    let calls = 0;
    const gateway: LibrarianAuditGateway = {
      runLibrarianAudit: () => {
        calls += 1;
        return calls === 1 ? Promise.resolve(PASS_REPORT) : rerun.promise;
      },
    };
    render(
      <LibrarianMonitorSection
        auditGateway={gateway}
        projectId="project-1"
      />,
    );

    expect(
      await screen.findByText("交接中的在途计数与台账一致。"),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "重新检查" }));

    expect(screen.getByRole("button", { name: "检查中…" })).toBeDisabled();
    expect(screen.queryByText("交接中的在途计数与台账一致。")).toBeNull();
    expect(screen.queryAllByText("PASS")).toHaveLength(0);

    rerun.resolve(FAIL_REPORT);
    expect(
      await screen.findByText("交接声称 0 项在途，但台账记录 2 项。"),
    ).toBeVisible();
    expect(screen.queryByText("交接中的在途计数与台账一致。")).toBeNull();
    expect(calls).toBe(2);
  });

  it("reports a rerun failure without showing the previous result or host detail", async () => {
    const user = userEvent.setup();
    const rerun = deferred<LibrarianAuditReport>();
    let calls = 0;
    const gateway: LibrarianAuditGateway = {
      runLibrarianAudit: () => {
        calls += 1;
        return calls === 1 ? Promise.resolve(PASS_REPORT) : rerun.promise;
      },
    };
    render(
      <LibrarianMonitorSection
        auditGateway={gateway}
        projectId="project-1"
      />,
    );

    expect(
      await screen.findByText("交接中的在途计数与台账一致。"),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "重新检查" }));
    rerun.reject(new Error("host detail must never reach the interface"));

    expect(
      await screen.findByRole("alert", {
        name: "本次检查失败。未显示上一次的结果。",
      }),
    ).toBeVisible();
    expect(screen.queryByText("交接中的在途计数与台账一致。")).toBeNull();
    expect(screen.queryAllByText("PASS")).toHaveLength(0);
    expect(screen.queryAllByText("FAIL")).toHaveLength(0);
    expect(screen.queryByText(/host detail/u)).toBeNull();
    expect(calls).toBe(2);
  });

  it("names the configured librarian as responsible and omits setup guidance", async () => {
    render(
      <LibrarianMonitorSection
        auditGateway={auditGateway(PASS_REPORT)}
        projectId="project-1"
        runtimeSlotsGateway={new MockRuntimeSlotsGateway([
          slot("librarian", "资料守望者"),
        ])}
      />,
    );

    expect(
      await screen.findByTestId("librarian-monitor-responsibility"),
    ).toHaveTextContent("由 资料守望者 负责");
    expect(screen.queryByTestId("librarian-monitor-no-role")).toBeNull();
    expect(screen.queryByText("librarian-slot")).toBeNull();
  });

  it("guides librarian setup while keeping the built-in audit available", async () => {
    let auditCalls = 0;
    const gateway: LibrarianAuditGateway = {
      runLibrarianAudit: async () => {
        auditCalls += 1;
        return PASS_REPORT;
      },
    };
    render(
      <LibrarianMonitorSection
        auditGateway={gateway}
        projectId="project-1"
        runtimeSlotsGateway={new MockRuntimeSlotsGateway([
          slot("controller", "项目总控"),
        ])}
      />,
    );

    const guidance = await screen.findByTestId("librarian-monitor-no-role");
    expect(guidance).toHaveTextContent("尚未设置信息管理员");
    expect(guidance).toHaveTextContent("体检仍然可用");
    expect(screen.queryByTestId("librarian-monitor-responsibility")).toBeNull();
    expect(await screen.findAllByText("PASS")).toHaveLength(4);
    expect(screen.getByRole("button", { name: "重新检查" })).toBeEnabled();
    expect(auditCalls).toBe(1);
  });

  it("does not let a late report from the previous project replace the current result", async () => {
    const delayedA = deferred<LibrarianAuditReport>();
    const reportA = reportForProject("project-a", "项目 A 的旧结果。");
    const reportB = reportForProject("project-b", "项目 B 的当前结果。");
    const gateway: LibrarianAuditGateway = {
      runLibrarianAudit: (projectId) => {
        if (projectId === "project-a") return delayedA.promise;
        if (projectId === "project-b") return Promise.resolve(reportB);
        return Promise.reject(new Error("unexpected project"));
      },
    };
    const { rerender } = render(
      <LibrarianMonitorSection
        auditGateway={gateway}
        projectId="project-a"
      />,
    );

    rerender(
      <LibrarianMonitorSection
        auditGateway={gateway}
        projectId="project-b"
      />,
    );
    expect(await screen.findByText("项目 B 的当前结果。")).toBeVisible();

    await act(async () => {
      delayedA.resolve(reportA);
      await delayedA.promise;
    });
    expect(screen.queryByText("项目 A 的旧结果。")).toBeNull();
    expect(screen.getByText("项目 B 的当前结果。")).toBeVisible();
  });

  it("shows no invented health status when no host audit gateway is available", async () => {
    render(
      <LibrarianMonitorSection
        projectId="project-1"
        runtimeSlotsGateway={new MockRuntimeSlotsGateway([])}
      />,
    );

    expect(
      await screen.findByTestId("librarian-monitor-host-unavailable"),
    ).toHaveTextContent("没有可展示的检查状态");
    expect(screen.queryAllByText("PASS")).toHaveLength(0);
    expect(screen.queryAllByText("FAIL")).toHaveLength(0);
    expect(screen.getByRole("button", { name: "重新检查" })).toBeDisabled();
  });

  it("keeps audit results visible when responsibility lookup fails", async () => {
    const slotsGateway = new MockRuntimeSlotsGateway([]);
    vi.spyOn(slotsGateway, "listRuntimeSlots").mockRejectedValueOnce(
      new Error("private host detail"),
    );
    render(
      <LibrarianMonitorSection
        auditGateway={auditGateway(PASS_REPORT)}
        projectId="project-1"
        runtimeSlotsGateway={slotsGateway}
      />,
    );

    expect(
      await screen.findByTestId("librarian-monitor-role-unavailable"),
    ).toHaveTextContent("不影响项目体检");
    expect(screen.queryByText(/private host detail/u)).toBeNull();
    expect(await screen.findAllByText("PASS")).toHaveLength(4);
    expect(screen.queryByTestId("librarian-monitor-no-role")).toBeNull();
    expect(screen.queryByTestId("librarian-monitor-responsibility")).toBeNull();
  });
});
