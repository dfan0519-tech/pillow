-- OAuth / OIDC slot auth: extend uniform slot model with oauth auth_kind.
-- Tokens stay out of this table: only DPAPI secret refs + non-secret metadata.
-- Existing rows are copied byte-for-byte (secret_ref unchanged).

ALTER TABLE runtime_slots RENAME TO runtime_slots_before_0018;

CREATE TABLE runtime_slots (
    slot_id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL
        REFERENCES projects(id) ON DELETE CASCADE,
    display_name TEXT NOT NULL
        CHECK (length(trim(display_name)) BETWEEN 1 AND 120),
    auth_kind TEXT NOT NULL
        CHECK (auth_kind IN ('api_key', 'grok_login', 'local_cli', 'oauth')),
    role_kind TEXT NOT NULL
        CHECK (role_kind IN ('controller', 'librarian', 'custom')),
    role_custom_name TEXT,
    base_url TEXT,
    model_id TEXT,
    secret_ref TEXT,
    executable_path TEXT,
    launch_args_json TEXT,
    status_args_json TEXT,
    refresh_secret_ref TEXT,
    access_expires_at TEXT,
    oauth_meta_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (
        (
            auth_kind = 'grok_login'
            AND base_url IS NULL
            AND secret_ref IS NULL
            AND executable_path IS NULL
            AND launch_args_json IS NULL
            AND status_args_json IS NULL
            AND refresh_secret_ref IS NULL
            AND access_expires_at IS NULL
            AND oauth_meta_json IS NULL
            AND (model_id IS NULL OR length(trim(model_id)) > 0)
        )
        OR
        (
            auth_kind = 'api_key'
            AND length(trim(base_url)) > 0
            AND length(trim(model_id)) > 0
            AND length(trim(secret_ref)) > 0
            AND executable_path IS NULL
            AND launch_args_json IS NULL
            AND status_args_json IS NULL
            AND refresh_secret_ref IS NULL
            AND access_expires_at IS NULL
            AND oauth_meta_json IS NULL
        )
        OR
        (
            auth_kind = 'local_cli'
            AND base_url IS NULL
            AND model_id IS NULL
            AND secret_ref IS NULL
            AND length(trim(executable_path)) > 0
            AND launch_args_json IS NOT NULL
            AND status_args_json IS NOT NULL
            AND refresh_secret_ref IS NULL
            AND access_expires_at IS NULL
            AND oauth_meta_json IS NULL
        )
        OR
        (
            auth_kind = 'oauth'
            AND length(trim(base_url)) > 0
            AND length(trim(model_id)) > 0
            AND length(trim(secret_ref)) > 0
            AND length(trim(refresh_secret_ref)) > 0
            AND length(trim(access_expires_at)) > 0
            AND length(trim(oauth_meta_json)) > 0
            AND executable_path IS NULL
            AND launch_args_json IS NULL
            AND status_args_json IS NULL
        )
    ),
    CHECK (
        (
            role_kind = 'custom'
            AND role_custom_name IS NOT NULL
            AND length(trim(role_custom_name)) BETWEEN 1 AND 64
        )
        OR
        (
            role_kind IN ('controller', 'librarian')
            AND role_custom_name IS NULL
        )
    )
);

INSERT INTO runtime_slots (
    slot_id, project_id, display_name, auth_kind, role_kind, role_custom_name,
    base_url, model_id, secret_ref, executable_path,
    launch_args_json, status_args_json,
    refresh_secret_ref, access_expires_at, oauth_meta_json,
    created_at, updated_at
)
SELECT
    slot_id,
    project_id,
    display_name,
    auth_kind,
    role_kind,
    role_custom_name,
    base_url,
    model_id,
    secret_ref,
    executable_path,
    launch_args_json,
    status_args_json,
    NULL,
    NULL,
    NULL,
    created_at,
    updated_at
FROM runtime_slots_before_0018;

DROP TABLE runtime_slots_before_0018;

CREATE INDEX idx_runtime_slots_project_created
    ON runtime_slots(project_id, created_at ASC, slot_id ASC);

CREATE UNIQUE INDEX idx_runtime_slots_one_controller
    ON runtime_slots(project_id)
    WHERE role_kind = 'controller';
