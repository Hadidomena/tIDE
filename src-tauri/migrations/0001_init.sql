-- 0001_init.sql — tIDE SQLite schema
-- Verbatim copy of plan-v2 §8 (notes/plan-v2.md). Timestamps: ISO-8601 TEXT.
-- NOTE: the FTS tables below are contentless placeholders; when the analyzer/search
-- features land (M3/M5), revisit with proper external-content FTS + sync triggers.

-- (1) environment settings — key/value per §3.1
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL);
-- keys: engine, ghc_path_override, sclang_path_override, sample_dirs_json,
--       ngram_min, ngram_max, editor_theme, last_project_id ...

-- (2) projects (was missing in v1 draft)
CREATE TABLE projects (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT NOT NULL,
  dir_path TEXT NOT NULL UNIQUE,          -- folder holding the .tidal files
  entry_file TEXT NOT NULL DEFAULT 'main.tidal',
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);

-- (3) reusable snippets/patterns
CREATE TABLE snippets (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT NOT NULL,
  code_content TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('snippet','pattern','template')),
  source_project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL,
  use_count INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL, updated_at TEXT NOT NULL
);

-- (4) normalized tags + joins (replaces comma-separated v1 tags)
CREATE TABLE tags (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE);
CREATE TABLE snippet_tags (snippet_id INTEGER NOT NULL REFERENCES snippets(id) ON DELETE CASCADE,
                           tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                           PRIMARY KEY (snippet_id, tag_id));
CREATE TABLE sample_tags  (sample_id INTEGER NOT NULL REFERENCES samples(id) ON DELETE CASCADE,
                           tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                           PRIMARY KEY (sample_id, tag_id));

-- (5) sample library
CREATE TABLE samples (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  file_path TEXT NOT NULL UNIQUE,
  hash TEXT NOT NULL,                     -- blake3, dedupe
  source_type TEXT NOT NULL CHECK (source_type IN ('community','custom','derived')),
  parent_id INTEGER REFERENCES samples(id) ON DELETE SET NULL,  -- if derived
  duration_ms INTEGER, sample_rate INTEGER, channels INTEGER,
  file_size INTEGER, is_stale INTEGER NOT NULL DEFAULT 0,       -- external edit/delete
  created_at TEXT NOT NULL
);

-- (6) transform audit (spec §3.3.3)
CREATE TABLE sample_transformations (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  sample_id INTEGER NOT NULL REFERENCES samples(id) ON DELETE CASCADE,   -- source
  result_sample_id INTEGER REFERENCES samples(id) ON DELETE SET NULL,    -- derived output
  operation_type TEXT NOT NULL CHECK (operation_type IN
    ('reverse','trim','normalize','gain','fade')),
  parameters_json TEXT NOT NULL,          -- {"start_ms":100,"end_ms":1500}
  created_at TEXT NOT NULL
);

-- (7) analyzer output (spec §3.2)
CREATE TABLE pattern_occurrences (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  path_hash TEXT NOT NULL,                -- normalized repeated token sequence
  code TEXT NOT NULL,                     -- representative snippet
  file_path TEXT NOT NULL, start_line INTEGER, end_line INTEGER,
  occurrences INTEGER NOT NULL, analyzed_at TEXT NOT NULL
);

-- (8) full-text search (external-content FTS over snippets & samples)
CREATE VIRTUAL TABLE snippets_fts USING fts5(title, code_content, tags, content='');
CREATE VIRTUAL TABLE samples_fts   USING fts5(name, tags, file_path, content='');
CREATE INDEX idx_samples_hash ON samples(hash);
CREATE INDEX idx_occ_path     ON pattern_occurrences(project_id, path_hash);
