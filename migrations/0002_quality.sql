-- SPDX-License-Identifier: GPL-3.0-only
-- Copyright (C) 2026 Vaibhav
--
-- Per-bookmark quality score (0..1) computed at crawl time from extracted text
-- length and boilerplate ratio. Used by the re-ranker and shown in the UI so
-- thin/junk pages sort below real articles.

ALTER TABLE bookmarks ADD COLUMN quality REAL NOT NULL DEFAULT 1.0;
CREATE INDEX IF NOT EXISTS bookmarks_user_quality ON bookmarks(user_id, quality);
