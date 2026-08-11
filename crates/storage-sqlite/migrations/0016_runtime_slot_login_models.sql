ALTER TABLE runtime_slots RENAME TO runtime_slots_before_0016;

CREATE TABLE runtime_slots (
    slot_id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL
        REFERENCES projects(id) ON DELETE CASCADE,
    display_name TEXT NOT NULL
        CHECK (length(trim(display_name)) BETWEEN 1 AND 120),
    kind TEXT NOT NULL
        CHECK (kind IN ('governed_grok', 'external_cli')),
    governance TEXT NOT NULL
        CHECK (governance IN ('full', 'none')),
    auth_mode TEXT
        CHECK (auth_mode IS NULL OR auth_mode IN ('login', 'api_key')),
    base_url TEXT,
    model_id TEXT,
    secret_ref TEXT,
    executable_path TEXT,
    launch_args_json TEXT,
    status_args_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (
        (
            kind = 'governed_grok'
            AND governance = 'full'
            AND executable_path IS NULL
            AND launch_args_json IS NULL
            AND status_args_json IS NULL
            AND (
                (
                    auth_mode = 'login'
                    AND base_url IS NULL
                    AND (model_id IS NULL OR length(trim(model_id)) > 0)
                    AND secret_ref IS NULL
                )
                OR
                (
                    auth_mode = 'api_key'
                    AND length(trim(base_url)) > 0
                    AND length(trim(model_id)) > 0
                    AND length(trim(secret_ref)) > 0
                )
            )
        )
        OR
        (
            kind = 'external_cli'
            AND governance = 'none'
            AND auth_mode IS NULL
            AND base_url IS NULL
            AND model_id IS NULL
            AND secret_ref IS NULL
            AND length(trim(executable_path)) > 0
            AND launch_args_json IS NOT NULL
            AND status_args_json IS NOT NULL
        )
    )
);

INSERT INTO runtime_slots (
    slot_id, project_id, display_name, kind, governance, auth_mode,
    base_url, model_id, secret_ref, executable_path,
    launch_args_json, status_args_json, created_at, updated_at
)
SELECT
    slot_id, project_id, display_name, kind, governance, auth_mode,
    base_url, model_id, secret_ref, executable_path,
    launch_args_json, status_args_json, created_at, updated_at
FROM runtime_slots_before_0016;

DROP TABLE runtime_slots_before_0016;

CREATE INDEX idx_runtime_slots_project_created
    ON runtime_slots(project_id, created_at ASC, slot_id ASC);
