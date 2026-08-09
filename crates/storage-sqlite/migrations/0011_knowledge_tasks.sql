-- K0d dispatch / acceptance ledger (storage only).
-- Registration is a byproduct of dispatch and acceptance (ADR-0004 §2).
-- No lease ids, session ids, or machine user paths — only slot labels.
--
-- SQL CHECK enforces shape and enum membership.
-- Value validation (portable brief_path via keeper_project SSoT, privacy
-- red lines for lease/session/host paths) lives in Rust host code —
-- same split as 0009_settings.sql / 0010_knowledge.sql.

CREATE TABLE IF NOT EXISTS knowledge_tasks (
    task_id TEXT PRIMARY KEY NOT NULL
        CHECK (length(task_id) >= 1 AND length(task_id) <= 64),
    project_id TEXT NOT NULL REFERENCES projects(id),
    title TEXT NOT NULL
        CHECK (length(title) >= 1 AND length(title) <= 512),
    brief_path TEXT NULL
        CHECK (brief_path IS NULL OR (
            length(brief_path) >= 1 AND length(brief_path) <= 4096
        )),
    -- Slot label only (e.g. grok3). Never a lease or session id.
    worker_label TEXT NOT NULL
        CHECK (length(worker_label) >= 1 AND length(worker_label) <= 64),
    dispatched_at TEXT NOT NULL
        CHECK (length(dispatched_at) >= 1),
    status TEXT NOT NULL DEFAULT 'dispatched'
        CHECK (status IN (
            'dispatched',
            'completed',
            'blocked',
            'abandoned'
        )),
    completion_signal TEXT NULL
        CHECK (completion_signal IS NULL OR (
            length(completion_signal) >= 1 AND length(completion_signal) <= 8192
        ))
);

CREATE INDEX IF NOT EXISTS idx_knowledge_tasks_project_id
    ON knowledge_tasks(project_id);

CREATE INDEX IF NOT EXISTS idx_knowledge_tasks_project_status
    ON knowledge_tasks(project_id, status);

CREATE INDEX IF NOT EXISTS idx_knowledge_tasks_project_dispatched
    ON knowledge_tasks(project_id, dispatched_at);

-- Controller acceptance verdict. task_id is optional: the controller may
-- record a verdict for work it did itself (no dispatch row).
CREATE TABLE IF NOT EXISTS knowledge_verdicts (
    verdict_id TEXT PRIMARY KEY NOT NULL
        CHECK (length(verdict_id) >= 1 AND length(verdict_id) <= 64),
    task_id TEXT NULL
        REFERENCES knowledge_tasks(task_id),
    project_id TEXT NOT NULL REFERENCES projects(id),
    outcome TEXT NOT NULL
        CHECK (outcome IN (
            'accepted',
            'rejected',
            'partial'
        )),
    gate_result TEXT NULL
        CHECK (gate_result IS NULL OR (
            length(gate_result) >= 1 AND length(gate_result) <= 512
        )),
    e2e_result TEXT NULL
        CHECK (e2e_result IS NULL OR (
            length(e2e_result) >= 1 AND length(e2e_result) <= 512
        )),
    -- The highest-value column in the knowledge base (controller judgment).
    summary TEXT NOT NULL
        CHECK (length(summary) >= 1 AND length(summary) <= 8192),
    residuals TEXT NULL
        CHECK (residuals IS NULL OR (
            length(residuals) >= 1 AND length(residuals) <= 8192
        )),
    decided_at TEXT NOT NULL
        CHECK (length(decided_at) >= 1)
);

CREATE INDEX IF NOT EXISTS idx_knowledge_verdicts_project_id
    ON knowledge_verdicts(project_id);

CREATE INDEX IF NOT EXISTS idx_knowledge_verdicts_project_decided
    ON knowledge_verdicts(project_id, decided_at);

CREATE INDEX IF NOT EXISTS idx_knowledge_verdicts_task_id
    ON knowledge_verdicts(task_id)
    WHERE task_id IS NOT NULL;

-- Which registry documents a task produced (optional many-to-many).
CREATE TABLE IF NOT EXISTS knowledge_task_documents (
    task_id TEXT NOT NULL
        REFERENCES knowledge_tasks(task_id),
    document_id TEXT NOT NULL
        REFERENCES knowledge_documents(document_id),
    PRIMARY KEY (task_id, document_id)
);

CREATE INDEX IF NOT EXISTS idx_knowledge_task_documents_document
    ON knowledge_task_documents(document_id);
