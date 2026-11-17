-- SPDX-License-Identifier: GPL-3.0-only
-- Copyright (C) 2026 Vaibhav
--
-- Schema v1. Forward-only migrations: bump `meta` schema_version and add a new
-- `000N_*.sql` file. Applied on first open; idempotent because of the
-- `user_version` gate in `bk_db::migrate`.

CREATE TABLE IF NOT EXISTS users (
    id              INTEGER PRIMARY KEY,
    username        TEXT NOT NULL UNIQUE,
    pw_hash         TEXT NOT NULL,
    role            TEXT NOT NULL DEFAULT 'user',
    created_at      INTEGER NOT NULL,
    settings_json   TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS sessions (
    id          INTEGER PRIMARY KEY,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  TEXT NOT NULL UNIQUE,
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL,
    ip          TEXT,
    ua           TEXT
);

CREATE TABLE IF NOT EXISTS bookmarks (
    id          INTEGER PRIMARY KEY,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    url_norm    TEXT NOT NULL,
    url         TEXT NOT NULL,
    title       TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    text        TEXT NOT NULL DEFAULT '',
    text_hash   INTEGER NOT NULL DEFAULT 0,
    lang        TEXT,
    site        TEXT NOT NULL DEFAULT '',
    status      TEXT NOT NULL DEFAULT 'new',
    note        TEXT NOT NULL DEFAULT '',
    pinned      INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    crawled_at  INTEGER,
    UNIQUE (user_id, url_norm)
);

CREATE TABLE IF NOT EXISTS tags (
    id          INTEGER PRIMARY KEY,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    color       TEXT NOT NULL DEFAULT '',
    UNIQUE (user_id, name)
);

CREATE TABLE IF NOT EXISTS bookmark_tags (
    bookmark_id INTEGER NOT NULL REFERENCES bookmarks(id) ON DELETE CASCADE,
    tag_id      INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (bookmark_id, tag_id)
);

CREATE TABLE IF NOT EXISTS screenshots (
    bookmark_id INTEGER PRIMARY KEY REFERENCES bookmarks(id) ON DELETE CASCADE,
    mime        TEXT NOT NULL,
    width       INTEGER NOT NULL,
    height      INTEGER NOT NULL,
    bytes       INTEGER NOT NULL,
    sha256      TEXT NOT NULL,
    blob        BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS crawls (
    id          INTEGER PRIMARY KEY,
    bookmark_id INTEGER NOT NULL UNIQUE REFERENCES bookmarks(id) ON DELETE CASCADE,
    http_status INTEGER,
    error_kind  TEXT,
    attempts   INTEGER NOT NULL DEFAULT 0,
    next_try_at INTEGER NOT NULL DEFAULT 0,
    bytes       INTEGER NOT NULL DEFAULT 0,
    ms          INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS vectors (
    bookmark_id INTEGER NOT NULL REFERENCES bookmarks(id) ON DELETE CASCADE,
    model_id    TEXT NOT NULL,
    dim         INTEGER NOT NULL,
    quant       TEXT NOT NULL DEFAULT 'int8',
    scale       REAL NOT NULL DEFAULT 1,
    blob        BLOB NOT NULL,
    PRIMARY KEY (bookmark_id, model_id)
);

CREATE TABLE IF NOT EXISTS clicks (
    id          INTEGER PRIMARY KEY,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    query       TEXT NOT NULL,
    bookmark_id INTEGER NOT NULL REFERENCES bookmarks(id) ON DELETE CASCADE,
    rank        INTEGER NOT NULL,
    ts          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS shares (
    id          INTEGER PRIMARY KEY,
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    bookmark_id INTEGER NOT NULL REFERENCES bookmarks(id) ON DELETE CASCADE,
    token       TEXT NOT NULL UNIQUE,
    expires_at  INTEGER
);

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS bookmarks_user_created ON bookmarks(user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS bookmarks_user_site   ON bookmarks(user_id, site);
CREATE INDEX IF NOT EXISTS bookmarks_user_url    ON bookmarks(user_id, url_norm);
CREATE INDEX IF NOT EXISTS vectors_model         ON vectors(model_id);
CREATE INDEX IF NOT EXISTS clicks_user_ts        ON clicks(user_id, ts);
CREATE INDEX IF NOT EXISTS crawls_due           ON crawls(next_try_at);

-- FTS5 on the bookmark text fields. External-content so the index stays in
-- sync via triggers; unicode61 + remove_diacritics for fuzzy matching.
CREATE VIRTUAL TABLE IF NOT EXISTS bookmark_fts USING fts5(
    title, description, text,
    content = 'bookmarks', content_rowid = 'id',
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS bookmarks_ai AFTER INSERT ON bookmarks BEGIN
    INSERT INTO bookmark_fts(rowid, title, description, text)
    VALUES (new.id, new.title, new.description, new.text);
END;

CREATE TRIGGER IF NOT EXISTS bookmarks_ad AFTER DELETE ON bookmarks BEGIN
    INSERT INTO bookmark_fts(bookmark_fts, rowid, title, description, text)
    VALUES ('delete', old.id, old.title, old.description, old.text);
END;

CREATE TRIGGER IF NOT EXISTS bookmarks_au AFTER UPDATE ON bookmarks BEGIN
    INSERT INTO bookmark_fts(bookmark_fts, rowid, title, description, text)
    VALUES ('delete', old.id, old.title, old.description, old.text);
    INSERT INTO bookmark_fts(rowid, title, description, text)
    VALUES (new.id, new.title, new.description, new.text);
END;

-- Trigram index for substring queries (3+ chars). 1-2 char queries fall back
-- to LIKE, which is handled in bk_search. External-content, same triggers.
CREATE VIRTUAL TABLE IF NOT EXISTS fts_tri USING fts5(
    title, text,
    content = 'bookmarks', content_rowid = 'id',
    tokenize = 'trigram'
);

CREATE TRIGGER IF NOT EXISTS bookmarks_tri_ai AFTER INSERT ON bookmarks BEGIN
    INSERT INTO fts_tri(rowid, title, text) VALUES (new.id, new.title, new.text);
END;
CREATE TRIGGER IF NOT EXISTS bookmarks_tri_ad AFTER DELETE ON bookmarks BEGIN
    INSERT INTO fts_tri(fts_tri, rowid, title, text) VALUES ('delete', old.id, old.title, old.text);
END;
CREATE TRIGGER IF NOT EXISTS bookmarks_tri_au AFTER UPDATE ON bookmarks BEGIN
    INSERT INTO fts_tri(fts_tri, rowid, title, text) VALUES ('delete', old.id, old.title, old.text);
    INSERT INTO fts_tri(rowid, title, text) VALUES (new.id, new.title, new.text);
END;

-- Title hits matter far more than body hits (10x / 2x / 1x).
INSERT INTO bookmark_fts(bookmark_fts, rank) VALUES('rank', 'bm25(10.0, 2.0, 1.0)');

INSERT OR IGNORE INTO meta (key, value) VALUES ('schema_version', '1');
INSERT OR IGNORE INTO meta (key, value) VALUES ('created_at', CAST(strftime('%s','now') AS TEXT));