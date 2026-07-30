/**
 * Shared fixtures for Slice B1 conversation workbench renderer tests.
 * Test-only helpers — not production code.
 */
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "../src/App";
import { MockConversationGateway } from "../src/lib/mockConversationGateway";
import type { ConversationGateway } from "../src/lib/conversationGateway";
import {
  ProjectGatewayError,
  type ProjectDto,
  type ProjectGateway,
} from "../src/lib/projectGateway";

export function fixtureProject(overrides: Partial<ProjectDto> = {}): ProjectDto {
  return {
    id: "project-a",
    name: "Project A",
    rootPath: "F:\\work\\project-a",
    trust: "trusted",
    createdAt: "2026-07-29T00:00:00.000Z",
    updatedAt: "2026-07-29T00:00:00.000Z",
    ...overrides,
  };
}

export function fixtureProjectGateway(
  projects: ProjectDto[] = [fixtureProject()],
  options: {
    openProject?: ProjectDto;
  } = {},
): ProjectGateway {
  const store = [...projects];
  return {
    listProjects: async () => [...store],
    createProject: async (input) => {
      const project = fixtureProject({
        id: crypto.randomUUID(),
        name: input.name,
        rootPath: input.rootPath,
      });
      store.push(project);
      return project;
    },
    selectProject: async (projectId) => {
      const project = store.find((item) => item.id === projectId);
      if (!project) {
        throw new ProjectGatewayError("project_not_found", "Project not found.");
      }
      return project;
    },
    pickDirectory: async () => "F:\\work\\picked",
    openOrRegisterProject: async () => {
      const project =
        options.openProject
        ?? fixtureProject({
          id: "opened-project",
          name: "Opened Project",
          rootPath: "F:\\work\\opened",
        });
      if (!store.some((item) => item.id === project.id)) store.push(project);
      return project;
    },
  };
}

export function renderApp(
  conversationGateway: ConversationGateway,
  projects: ProjectDto[] = [fixtureProject()],
  projectGateway?: ProjectGateway,
) {
  return render(
    <App
      conversationGateway={conversationGateway}
      gateway={projectGateway ?? fixtureProjectGateway(projects)}
    />,
  );
}

export async function selectProject(name = "Project A") {
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name }));
  return user;
}

export async function drainMock(gateway: MockConversationGateway) {
  await act(async () => {
    await gateway.drain();
  });
}

export async function selectProjectAfterRender(gateway: ConversationGateway) {
  renderApp(gateway);
  return selectProject("Project A");
}
