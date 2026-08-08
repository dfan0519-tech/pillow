-- K0a knowledge document registry (storage only).
-- Domain authority for document role, lifecycle status, and multi-valued
-- supersession / related / implements links. No scan, no retirement, no UI.
--
-- SQL CHECK enforces shape, enum membership, and link endpoint non-emptiness.
-- Value validation (portable path via keeper_project SSoT, digest format)
-- lives in Rust host code — same split as 0009_settings.sql.
-- No secret columns.

CREATE TABLE IF NOT EXISTS knowledge_documents (
    document_id TEXT PRIMARY KEY NOT NULL
        CHECK (length(document_id) >= 1 AND length(document_id) <= 64),
    project_id TEXT NOT NULL REFERENCES projects(id),
    path TEXT NOT NULL
        CHECK (length(path) >= 1 AND length(path) <= 4096),
    role TEXT NOT NULL
        CHECK (role IN (
            'brief',
            'report',
            'decision',
            'research',
            'verification',
            'spec',
            'plan',
            'handoff',
            'roadmap',
            'migration',
            'reference',
            'other'
        )),
    status TEXT NOT NULL DEFAULT 'unknown'
        CHECK (status IN (
            'active',
            'proposed',
            'accepted',
            'consumed',
            'superseded',
            'evidence',
            'unknown',
            'archived'
        )),
    content_digest TEXT NOT NULL
        CHECK (length(content_digest) >= 1 AND length(content_digest) <= 128),
    updated_at TEXT NOT NULL
        CHECK (length(updated_at) >= 1),
    registered_by TEXT NOT NULL
        CHECK (registered_by IN ('controller', 'worker', 'lint')),
    source_run_id TEXT NULL
        CHECK (source_run_id IS NULL OR (
            length(source_run_id) >= 1 AND length(source_run_id) <= 64
        )),
    UNIQUE (project_id, path)
);

CREATE INDEX IF NOT EXISTS idx_knowledge_documents_project_id
    ON knowledge_documents(project_id);

CREATE INDEX IF NOT EXISTS idx_knowledge_documents_project_path
    ON knowledge_documents(project_id, path);

-- Multi-valued document links (1→N supersession is a first-class case).
-- to_document_id may be NULL when the target is not yet registered or cannot
-- be resolved; to_path may hold the original declared target (including free
-- text for non-path supersession claims). At least one endpoint is required.
CREATE TABLE IF NOT EXISTS knowledge_document_links (
    link_id TEXT PRIMARY KEY NOT NULL
        CHECK (length(link_id) >= 1 AND length(link_id) <= 64),
    from_document_id TEXT NOT NULL
        REFERENCES knowledge_documents(document_id),
    to_document_id TEXT NULL
        REFERENCES knowledge_documents(document_id),
    to_path TEXT NULL
        CHECK (to_path IS NULL OR (
            length(to_path) >= 1 AND length(to_path) <= 4096
        )),
    rel TEXT NOT NULL
        CHECK (rel IN ('supersedes', 'related', 'implements')),
    CHECK (
        to_document_id IS NOT NULL
        OR to_path IS NOT NULL
    )
);

CREATE INDEX IF NOT EXISTS idx_knowledge_document_links_from
    ON knowledge_document_links(from_document_id);

CREATE INDEX IF NOT EXISTS idx_knowledge_document_links_to_document
    ON knowledge_document_links(to_document_id)
    WHERE to_document_id IS NOT NULL;
