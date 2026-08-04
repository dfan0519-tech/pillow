-- C2-A durable project-scoped recent resource paths.
-- Filesystem authorization remains with ProjectResourceCapability; this table
-- only stores portable relative paths keyed by project.

CREATE TABLE IF NOT EXISTS resource_recents (
    project_id TEXT NOT NULL,
    path TEXT NOT NULL,
    opened_at TEXT NOT NULL,
    PRIMARY KEY (project_id, path)
);

CREATE INDEX IF NOT EXISTS idx_resource_recents_project_opened
    ON resource_recents(project_id, opened_at DESC, path ASC);
