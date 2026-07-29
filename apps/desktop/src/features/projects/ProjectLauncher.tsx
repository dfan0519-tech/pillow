import { FormEvent, useState } from "react";
import type { CreateProjectInput, ProjectDto } from "../../lib/projectGateway";

type ProjectLauncherProps = {
  projects: ProjectDto[];
  onCreate: (input: CreateProjectInput) => Promise<void>;
  onSelect: (projectId: string) => Promise<void>;
};

export function ProjectLauncher({ projects, onCreate, onSelect }: ProjectLauncherProps) {
  const [formOpen, setFormOpen] = useState(projects.length === 0);
  const [name, setName] = useState("");
  const [rootPath, setRootPath] = useState("");
  const [submitting, setSubmitting] = useState(false);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!name.trim() || !rootPath.trim()) return;
    setSubmitting(true);
    try {
      await onCreate({ name: name.trim(), rootPath: rootPath.trim() });
      setName("");
      setRootPath("");
      setFormOpen(false);
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <main className="launcher" aria-label="项目启动器">
      <section className="launcher__intro">
        <p className="eyebrow">本地工作区</p>
        <h1>打开项目，继续工作。</h1>
        <p>项目承载对话、文件、终端与后续的审批记录。</p>
      </section>
      <section className="launcher__projects" aria-label="最近项目">
        <div className="section-heading">
          <h2>项目</h2>
          <button onClick={() => setFormOpen(true)} type="button">新建项目</button>
        </div>
        {projects.length === 0 ? (
          <p className="empty-copy">还没有项目。先打开一个本地文件夹。</p>
        ) : (
          <ul className="project-cards">
            {projects.map((project) => (
              <li key={project.id}>
                <button aria-label={project.name} onClick={() => void onSelect(project.id)} type="button">
                  <strong>{project.name}</strong>
                  <span>{project.rootPath}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
      {formOpen ? (
        <form className="project-form" onSubmit={(event) => void submit(event)}>
          <label>项目名称<input onChange={(event) => setName(event.target.value)} value={name} /></label>
          <label>项目目录<input onChange={(event) => setRootPath(event.target.value)} value={rootPath} /></label>
          <div className="form-actions">
            <button onClick={() => setFormOpen(false)} type="button">取消</button>
            <button disabled={submitting} type="submit">创建</button>
          </div>
        </form>
      ) : null}
    </main>
  );
}
