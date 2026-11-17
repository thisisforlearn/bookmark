// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Search pipeline: parse → correct → lexical + semantic → fuse → re-rank →
//! snippet. Every stage degrades gracefully when a model or index is missing.
#![allow(clippy::needless_range_loop)]

pub mod parse;
pub mod seed_words;
pub mod suggest;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;

use bk_core::{Id, Result};
use bk_db::Db;
use bk_db::bookmarks::{self, Bookmark};
use bk_infer::{ModelSet, Scratch};

pub use parse::{ParsedQuery, parse as parse_query};
pub use suggest::SymSpell;

/// Tunables. Mirrors `bk_core::Config` search flags.
#[derive(Clone, Debug)]
pub struct SearchConfig {
    pub spell: bool,
    pub semantic: bool,
    pub rerank: bool,
    pub lexical_weight: f32,
    pub semantic_weight: f32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            spell: true,
            semantic: true,
            rerank: true,
            lexical_weight: 0.6,
            semantic_weight: 0.4,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SearchRequest {
    pub q: String,
    pub user_id: Id,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Clone, Debug, Default)]
pub struct Breakdown {
    pub spell_us: u32,
    pub lexical_us: u32,
    pub vector_us: u32,
    pub rerank_us: u32,
    pub total_us: u32,
}

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub bookmark: Bookmark,
    pub score: f32,
    pub bm25: f32,
    pub vector_sim: Option<f32>,
    pub snippet: String,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
    pub corrected_query: Option<String>,
    pub took_ms: u32,
    pub degraded: bool,
    pub breakdown: Breakdown,
    pub candidates: usize,
}

/// The search engine. Cheap to clone via `Arc`.
pub struct Searcher {
    db: Db,
    models: Arc<ModelSet>,
    cfg: SearchConfig,
    vocab: DashMap<Id, Arc<SymSpell>>,
    /// Decoded embedding vectors per user, so the semantic arm does not re-read
    /// and re-decode the whole vector table on every query.
    vectors: DashMap<Id, Arc<Vec<bk_db::vectors::VectorRow>>>,
}

impl Searcher {
    pub fn new(db: Db, models: Arc<ModelSet>, cfg: SearchConfig) -> Arc<Self> {
        Arc::new(Self {
            db,
            models,
            cfg,
            vocab: DashMap::new(),
            vectors: DashMap::new(),
        })
    }

    pub fn models(&self) -> &Arc<ModelSet> {
        &self.models
    }

    pub fn cfg(&self) -> &SearchConfig {
        &self.cfg
    }

    /// Drop the cached vocabulary for a user (call after imports/crawls).
    pub fn refresh_vocab(&self, user_id: Id) {
        self.vocab.remove(&user_id);
        self.vectors.remove(&user_id);
    }

    /// Drop every cached vector set (after a bulk re-embed).
    pub fn invalidate_vectors(&self) {
        self.vectors.clear();
    }

    fn vocab_for(&self, user_id: Id) -> Arc<SymSpell> {
        if let Some(v) = self.vocab.get(&user_id) {
            return v.clone();
        }
        let built = Arc::new(self.build_vocab(user_id));
        self.vocab.insert(user_id, built.clone());
        built
    }

    fn build_vocab(&self, user_id: Id) -> SymSpell {
        let mut words: HashMap<String, u32> = HashMap::new();
        for (w, f) in seed_words::seed_words() {
            *words.entry(w.to_string()).or_insert(0) += f;
        }
        let _ = self.db.with_conn_sync(|conn| {
            let mut stmt = conn.prepare("SELECT title, text FROM bookmarks WHERE user_id = ?")?;
            let rows = stmt.query_map([user_id.inner()], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (title, text) = row?;
                add_words(&mut words, &title, 6);
                // Only index the first chunk of body text to keep build time bounded.
                let tail: String = text.chars().take(4000).collect();
                add_words(&mut words, &tail, 1);
            }
            let mut tstmt = conn.prepare("SELECT name FROM tags WHERE user_id = ?")?;
            let trows = tstmt.query_map([user_id.inner()], |r| r.get::<_, String>(0))?;
            for t in trows.flatten() {
                *words.entry(t.to_lowercase()).or_insert(0) += 10;
            }
            Ok(())
        });
        SymSpell::build(words)
    }

    /// Run a search. Blocking; call from `spawn_blocking` in async handlers.
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let started = Instant::now();
        let mut bd = Breakdown::default();

        // 1. Parse.
        let mut parsed = parse_query(&req.q)?;
        if parsed.is_empty() {
            return Ok(SearchResponse::default());
        }

        // 2. Spell correction.
        let spell_start = Instant::now();
        let mut corrected_query = None;
        if self.cfg.spell {
            let vocab = self.vocab_for(req.user_id);
            let mut changed = false;
            for term in parsed.terms.clone() {
                if vocab.contains(&term) || term.chars().count() <= 2 {
                    continue;
                }
                let cands = vocab.lookup(&term, 2);
                if cands.is_empty() {
                    continue;
                }
                // Blend SymSpell confidence with the trained spell model when it
                // is installed; SymSpell alone is the fallback.
                let mut scratch = Scratch::new();
                let mut best: Option<(String, f32)> = None;
                for (cand, sym_conf) in &cands {
                    let score = match &self.models.spell {
                        Some(m) => 0.45 * sym_conf + 0.55 * m.score(&term, cand, &mut scratch),
                        None => *sym_conf,
                    };
                    if best.as_ref().is_none_or(|(_, b)| score > *b) {
                        best = Some((cand.clone(), score));
                    }
                }
                if let Some((best_word, conf)) = best
                    && conf >= 0.60
                    && best_word != term
                {
                    parsed.terms.retain(|t| t != &term);
                    parsed.terms.push(best_word);
                    changed = true;
                }
            }
            if changed {
                let mut cq = parsed.terms.join(" ");
                for p in &parsed.phrases {
                    cq.push_str(&format!(" \"{p}\""));
                }
                corrected_query = Some(cq.trim().to_string());
            }
        }
        bd.spell_us = spell_start.elapsed().as_micros() as u32;

        // 3. Lexical arm.
        let lex_start = Instant::now();
        let lexical = self.lexical(&parsed, req.user_id)?;
        bd.lexical_us = lex_start.elapsed().as_micros() as u32;

        // 4. Semantic arm.
        let vec_start = Instant::now();
        let semantic = if self.cfg.semantic {
            self.semantic(&req.q, req.user_id)?
        } else {
            Vec::new()
        };
        bd.vector_us = vec_start.elapsed().as_micros() as u32;

        // 5. Fuse with weighted RRF.
        let fused = self.fuse(&lexical, &semantic);
        let candidates = fused.len();

        // Take a generous candidate window for re-ranking.
        let window = (req.limit + req.offset).saturating_mul(4).clamp(50, 200);
        let mut ranked: Vec<(Id, f32, f32, Option<f32>)> = fused.into_iter().take(window).collect();

        // 6. Fetch + filter.
        let ids: Vec<Id> = ranked.iter().map(|(id, ..)| *id).collect();
        let fetched = self
            .db
            .with_conn_sync(|conn| bookmarks::get_many(conn, req.user_id, &ids))?;
        let mut by_id: HashMap<Id, Bookmark> = fetched.into_iter().map(|b| (b.id, b)).collect();

        let shots = self
            .db
            .with_conn_sync(|conn| bk_db::screenshots::existing_ids(conn, &ids))?;
        let tag_map = self.tags_for(&ids)?;

        ranked.retain(|(id, ..)| match by_id.get(id) {
            Some(b) => matches_filters(
                b,
                &parsed,
                tag_map.get(id).map(|v| v.as_slice()).unwrap_or(&[]),
                shots.contains(id),
            ),
            None => false,
        });

        // 7. Re-rank.
        let rerank_start = Instant::now();
        let mut scratch = Scratch::new();
        let now = bk_core::now();
        for (id, score, bm25, sim) in ranked.iter_mut() {
            if let Some(b) = by_id.get(id) {
                let feats = features(
                    b,
                    *score,
                    *bm25,
                    *sim,
                    now,
                    tag_map.get(id).map(|v| v.as_slice()).unwrap_or(&[]),
                    shots.contains(id),
                    &parsed,
                );
                let final_score = if self.cfg.rerank {
                    match &self.models.rank {
                        Some(m) if m.in_dim() <= feats.len() => m.score(&feats, &mut scratch),
                        _ => heuristic(&feats),
                    }
                } else {
                    *score
                };
                *score = final_score;
            }
        }
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        bd.rerank_us = rerank_start.elapsed().as_micros() as u32;

        // 8. Build hits.
        let hits: Vec<SearchHit> = ranked
            .into_iter()
            .skip(req.offset)
            .take(req.limit)
            .filter_map(|(id, score, bm25, sim)| {
                let b = by_id.remove(&id)?;
                let snippet = make_snippet(&b, &parsed);
                Some(SearchHit {
                    tags: tag_map.get(&id).cloned().unwrap_or_default(),
                    bookmark: b,
                    score,
                    bm25,
                    vector_sim: sim,
                    snippet,
                })
            })
            .collect();

        bd.total_us = started.elapsed().as_micros() as u32;
        Ok(SearchResponse {
            hits,
            corrected_query,
            took_ms: (bd.total_us / 1000).max(1),
            degraded: self.models.degraded() || !self.cfg.semantic,
            breakdown: bd,
            candidates,
        })
    }

    fn tags_for(&self, ids: &[Id]) -> Result<HashMap<Id, Vec<String>>> {
        let mut out = HashMap::new();
        if ids.is_empty() {
            return Ok(out);
        }
        self.db.with_conn_sync(|conn| {
            for id in ids {
                let tags = bk_db::tags::for_bookmark(conn, *id)?;
                if !tags.is_empty() {
                    out.insert(*id, tags);
                }
            }
            Ok(())
        })?;
        Ok(out)
    }

    /// Lexical candidates: FTS5 AND, then OR, then trigram, then LIKE.
    fn lexical(&self, q: &ParsedQuery, user_id: Id) -> Result<Vec<(Id, f32)>> {
        let mut expr_parts: Vec<String> = Vec::new();
        for t in &q.terms {
            // Strip FTS5 special characters by quoting.
            expr_parts.push(bk_db::fts::quote(t));
        }
        for p in &q.phrases {
            expr_parts.push(bk_db::fts::quote(p));
        }
        let mut results: HashMap<Id, f32> = HashMap::new();

        if expr_parts.is_empty() {
            // Filter-only query (e.g. `tag:work`): seed with the user's library
            // and let the filters below narrow it.
            self.db.with_conn_sync(|conn| {
                let mut stmt = conn.prepare(
                    "SELECT id FROM bookmarks WHERE user_id = ? ORDER BY pinned DESC, created_at DESC LIMIT 500",
                )?;
                let rows = stmt.query_map([user_id.inner()], |r| Ok(Id::from_i64(r.get(0)?)))?;
                for id in rows.flatten() {
                    results.insert(id, 0.001);
                }
                Ok(())
            })?;
        }

        if !expr_parts.is_empty() {
            let and_expr = expr_parts.join(" AND ");
            let or_expr = expr_parts.join(" OR ");
            self.db.with_conn_sync(|conn| {
                let mut rows = bk_db::fts::match_fts(conn, user_id, &and_expr, 200)?;
                if rows.is_empty() {
                    rows = bk_db::fts::match_fts(conn, user_id, &or_expr, 200)?;
                }
                accumulate(&mut results, rows);
                // Trigram for substring matches (>= 3 chars).
                for t in &q.terms {
                    if t.chars().count() >= 3 {
                        if let Ok(rows) =
                            bk_db::fts::match_trigram(conn, user_id, &bk_db::fts::quote(t), 100)
                        {
                            accumulate(&mut results, rows);
                        }
                    } else if !t.is_empty()
                        && let Ok(rows) = bk_db::fts::like_fallback(conn, user_id, t, 50)
                    {
                        accumulate(&mut results, rows);
                    }
                }
                Ok(())
            })?;
        }

        let mut v: Vec<(Id, f32)> = results.into_iter().collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(v)
    }

    /// Semantic candidates via int8 dot product over the user's vectors.
    fn semantic(&self, query: &str, user_id: Id) -> Result<Vec<(Id, f32)>> {
        let model = match &self.models.embed {
            Some(m) => m,
            None => return Ok(Vec::new()),
        };
        let mut scratch = Scratch::new();
        let mut qbuf: Vec<u8> = Vec::new();
        let qscale = match model.encode(query, &mut qbuf, &mut scratch) {
            Ok(s) => s,
            Err(_) => return Ok(Vec::new()),
        };
        let qvec: Vec<i8> = qbuf.iter().map(|&b| b as i8).collect();
        let rows = self
            .db
            .with_conn_sync(|conn| bk_db::vectors::all_for_user(conn, user_id, "embed"))?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            if row.dim != qvec.len() {
                continue;
            }
            let dot = bk_infer::kernels::dot_i8(&qvec, &row.data) as f32;
            let cos = dot * qscale * row.scale;
            out.push((row.bookmark_id, cos));
        }
        out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        out.truncate(200);
        Ok(out)
    }

    fn fuse(
        &self,
        lexical: &[(Id, f32)],
        semantic: &[(Id, f32)],
    ) -> Vec<(Id, f32, f32, Option<f32>)> {
        const K: f32 = 60.0;
        let mut scores: HashMap<Id, (f32, f32, Option<f32>)> = HashMap::new();
        for (rank, (id, bm25)) in lexical.iter().enumerate() {
            let contrib = self.cfg.lexical_weight / (K + rank as f32 + 1.0);
            let e = scores.entry(*id).or_insert((0.0, *bm25, None));
            e.0 += contrib;
        }
        for (rank, (id, sim)) in semantic.iter().enumerate() {
            let contrib = self.cfg.semantic_weight / (K + rank as f32 + 1.0);
            let e = scores.entry(*id).or_insert((0.0, 0.0, None));
            e.0 += contrib;
            e.2 = Some(*sim);
        }
        let mut v: Vec<(Id, f32, f32, Option<f32>)> = scores
            .into_iter()
            .map(|(id, (s, bm, sim))| (id, s, bm, sim))
            .collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        v
    }
}

fn accumulate(map: &mut HashMap<Id, f32>, rows: Vec<(Id, f64)>) {
    for (rank, (id, score)) in rows.into_iter().enumerate() {
        // Convert bm25 (negative, lower better) into a positive rank score.
        let s = 1.0 / (60.0 + rank as f32 + 1.0) + (-score as f32).min(20.0) * 0.001;
        let e = map.entry(id).or_insert(0.0);
        *e += s;
    }
}

fn add_words(map: &mut HashMap<String, u32>, text: &str, weight: u32) {
    let mut cur = String::new();
    let mut count = 0;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            cur.extend(ch.to_lowercase());
        } else if !cur.is_empty() {
            if cur.len() >= 2 && cur.len() <= 40 {
                *map.entry(std::mem::take(&mut cur)).or_insert(0) += weight;
            } else {
                cur.clear();
            }
            count += 1;
            if count > 600 {
                break;
            }
        }
    }
    if !cur.is_empty() && cur.len() >= 2 {
        *map.entry(cur).or_insert(0) += weight;
    }
}

fn matches_filters(b: &Bookmark, q: &ParsedQuery, tags: &[String], has_shot: bool) -> bool {
    if q.screenshots_only && !has_shot {
        return false;
    }
    if !q.sites.is_empty() {
        let site = b.site.to_lowercase();
        if !q
            .sites
            .iter()
            .any(|s| site == *s || site.ends_with(&format!(".{s}")))
        {
            return false;
        }
    }
    if !q.tags.is_empty() {
        let lt: Vec<String> = tags.iter().map(|t| t.to_lowercase()).collect();
        if !q.tags.iter().all(|t| lt.contains(t)) {
            return false;
        }
    }
    if let Some(before) = q.before
        && b.created_at >= before
    {
        return false;
    }
    if let Some(after) = q.after
        && b.created_at < after
    {
        return false;
    }
    if !q.negated.is_empty() {
        let hay = format!(
            "{} {} {}",
            b.title.to_lowercase(),
            b.text.to_lowercase(),
            b.url.to_lowercase()
        );
        if q.negated.iter().any(|n| !n.is_empty() && hay.contains(n)) {
            return false;
        }
    }
    true
}

#[allow(clippy::too_many_arguments)]
fn features(
    b: &Bookmark,
    score: f32,
    _bm25: f32,
    sim: Option<f32>,
    now: i64,
    tags: &[String],
    has_shot: bool,
    q: &ParsedQuery,
) -> Vec<f32> {
    let age_days = ((now - b.created_at).max(0) as f32) / 86_400.0;
    let recency = (-age_days / 30.0).exp();
    let tag_match = if q.tags.is_empty() {
        0.0
    } else {
        let lt: Vec<String> = tags.iter().map(|t| t.to_lowercase()).collect();
        let hits = q.tags.iter().filter(|t| lt.contains(t)).count();
        hits as f32 / q.tags.len() as f32
    };
    let title_overlap = {
        let title = b.title.to_lowercase();
        if q.terms.is_empty() {
            0.0
        } else {
            let hits = q
                .terms
                .iter()
                .filter(|t| title.contains(t.as_str()))
                .count();
            hits as f32 / q.terms.len() as f32
        }
    };
    let domain_affinity = if b.site.is_empty() { 0.0 } else { 0.1 };
    vec![
        score.min(1.0),
        sim.unwrap_or(0.0).clamp(-1.0, 1.0),
        recency,
        if b.pinned { 1.0 } else { 0.0 },
        tag_match,
        domain_affinity,
        if has_shot { 1.0 } else { 0.0 },
        title_overlap,
        b.quality.clamp(0.0, 1.0),
    ]
}

fn heuristic(f: &[f32]) -> f32 {
    // Hand-tuned weights that work well without a trained re-ranker.
    let w = [1.0, 0.35, 0.20, 0.30, 0.25, 0.05, 0.03, 0.25, 0.35];
    f.iter().zip(w.iter()).map(|(a, b)| a * b).sum()
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Build an HTML snippet around the first query-term hit, with `<mark>` tags.
fn make_snippet(b: &Bookmark, q: &ParsedQuery) -> String {
    let source = if !b.description.is_empty() {
        &b.description
    } else if !b.text.is_empty() {
        &b.text
    } else {
        &b.title
    };
    let lower = source.to_lowercase();
    let mut best: Option<(usize, usize)> = None; // (start, len) in char indices
    for term in q.terms.iter().chain(q.phrases.iter()) {
        if term.is_empty() {
            continue;
        }
        if let Some(byte_pos) = lower.find(term.as_str()) {
            let start_chars = lower[..byte_pos].chars().count();
            let len_chars = term.chars().count();
            best = Some((start_chars, len_chars));
            break;
        }
    }
    let chars: Vec<char> = source.chars().collect();
    let (start, len) = best.unwrap_or_default();
    let window = 160usize;
    let from = start.saturating_sub(window / 3);
    let to = (from + window).min(chars.len());
    let mut text = String::new();
    if from > 0 {
        text.push('…');
    }
    let seg: String = chars[from..to].iter().collect();
    // Re-apply highlight on the byte-level segment.
    let esc = escape_html(&seg);
    if len > 0 && start >= from && start + len <= to {
        let local_start = start - from;
        let local_end = local_start + len;
        let pre: String = chars[from..from + local_start].iter().collect();
        let mid: String = chars[from + local_start..from + local_end].iter().collect();
        let post: String = chars[from + local_end..to].iter().collect();
        text.push_str(&escape_html(&pre));
        text.push_str("<mark>");
        text.push_str(&escape_html(&mid));
        text.push_str("</mark>");
        text.push_str(&escape_html(&post));
    } else {
        text.push_str(&esc);
    }
    if to < chars.len() {
        text.push('…');
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_highlights() {
        let b = Bookmark {
            title: "Rust".into(),
            text: "This is a long text about the rust programming language and its safety.".into(),
            ..Default::default()
        };
        let q = ParsedQuery {
            terms: vec!["rust".into()],
            ..Default::default()
        };
        let s = make_snippet(&b, &q);
        assert!(
            s.contains("<mark>rust</mark>") || s.contains("<mark>Rust</mark>"),
            "{s}"
        );
    }

    #[test]
    fn snippet_escapes_html() {
        let b = Bookmark {
            text: "<script>alert(1)</script> rust".into(),
            ..Default::default()
        };
        let q = ParsedQuery {
            terms: vec!["rust".into()],
            ..Default::default()
        };
        let s = make_snippet(&b, &q);
        assert!(!s.contains("<script>"));
        assert!(s.contains("&lt;script&gt;"));
    }

    #[test]
    fn filters() {
        let b = Bookmark {
            site: "example.com".into(),
            created_at: 100,
            ..Default::default()
        };
        let mut q = ParsedQuery::default();
        q.sites.push("example.com".into());
        assert!(matches_filters(&b, &q, &[], false));
        q.sites = vec!["other.com".into()];
        assert!(!matches_filters(&b, &q, &[], false));
    }
}
