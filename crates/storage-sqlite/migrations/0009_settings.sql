-- S0 non-secret settings (global + project scopes).
-- Domain authority for durable prefs lives here; env is allow-listed override only.
-- SQL CHECK enforces scope shape and non-null basics only.
-- Value validation (enums, paths, known keys) lives in Rust host code.
-- S0 has no secret columns; S1 credentials use a separate secret-ref store.

CREATE TABLE IF NOT EXISTS settings_entries (
    setting_id TEXT PRIMARY KEY NOT NULL
        CHECK (length(setting_id) >= 1 AND length(setting_id) <= 64),
    scope TEXT NOT NULL
        CHECK (scope IN ('global', 'project')),
    project_id TEXT NULL REFERENCES projects(id),
    key TEXT NOT NULL
        CHECK (length(key) >= 1 AND length(key) <= 256),
    value_kind TEXT NOT NULL
        CHECK (length(value_kind) >= 1 AND length(value_kind) <= 64),
    value_json TEXT NOT NULL
        CHECK (length(value_json) >= 1 AND length(value_json) <= 16384),
    updated_at TEXT NOT NULL
        CHECK (length(updated_at) >= 1),
    updated_by TEXT NOT NULL
        CHECK (length(updated_by) >= 1 AND length(updated_by) <= 64),
    CHECK (
        (scope = 'global' AND project_id IS NULL)
        OR (scope = 'project' AND project_id IS NOT NULL)
    )
);

-- SQLite treats NULLs as distinct in UNIQUE, so global uniqueness uses a
-- partial index on key alone; project uniqueness is (project_id, key).
CREATE UNIQUE INDEX IF NOT EXISTS idx_settings_entries_global_key
    ON settings_entries(key)
    WHERE scope = 'global';

CREATE UNIQUE INDEX IF NOT EXISTS idx_settings_entries_project_key
    ON settings_entries(project_id, key)
    WHERE scope = 'project';

CREATE INDEX IF NOT EXISTS idx_settings_entries_project_id
    ON settings_entries(project_id)
    WHERE project_id IS NOT NULL;
