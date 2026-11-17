// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! SymSpell: symmetric-delete typo correction. We hash delete variants into
//! `u64` keys (collisions are harmless because candidates are verified with a
//! real Levenshtein distance), which keeps memory bounded for a 20k vocabulary.

use std::collections::HashMap;

use rustc_hash::FxHashMap;

const MAX_WORDS: usize = 20_000;
const MAX_EDIT: usize = 2;

#[derive(Clone, Debug)]
struct Entry {
    word: String,
    freq: u32,
}

/// The index. Build once per user (or from a seed corpus) and share via `Arc`.
pub struct SymSpell {
    entries: Vec<Entry>,
    by_delete: FxHashMap<u64, Vec<u32>>,
    exact: FxHashMap<String, u32>,
}

impl SymSpell {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            by_delete: FxHashMap::default(),
            exact: FxHashMap::default(),
        }
    }

    /// Build from `(word, frequency)` pairs. Duplicates are summed.
    pub fn build<I: IntoIterator<Item = (String, u32)>>(words: I) -> Self {
        let mut freq: HashMap<String, u32> = HashMap::new();
        for (w, f) in words {
            let w = clean(&w);
            if w.len() < 2 || w.len() > 40 {
                continue;
            }
            *freq.entry(w).or_insert(0) += f.max(1);
        }
        let mut entries: Vec<Entry> = freq
            .into_iter()
            .map(|(word, freq)| Entry { word, freq })
            .collect();
        entries.sort_unstable_by(|a, b| b.freq.cmp(&a.freq).then_with(|| a.word.cmp(&b.word)));
        entries.truncate(MAX_WORDS);

        let mut by_delete: FxHashMap<u64, Vec<u32>> = FxHashMap::default();
        let mut exact = FxHashMap::default();
        for (i, e) in entries.iter().enumerate() {
            exact.insert(e.word.clone(), i as u32);
            for variant in deletes(&e.word, MAX_EDIT) {
                by_delete
                    .entry(bk_core::xxh3_str(&variant))
                    .or_default()
                    .push(i as u32);
            }
        }
        Self {
            entries,
            by_delete,
            exact,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, word: &str) -> bool {
        self.exact.contains_key(word)
    }

    /// Ranked correction candidates for `term` within edit distance `max_edit`.
    pub fn lookup(&self, term: &str, max_edit: usize) -> Vec<(String, f32)> {
        let term = clean(term);
        if term.is_empty() {
            return Vec::new();
        }
        if let Some(&idx) = self.exact.get(&term) {
            let e = &self.entries[idx as usize];
            return vec![(e.word.clone(), 1.0)];
        }
        let max_edit = max_edit.min(MAX_EDIT);
        let mut cand: Vec<u32> = Vec::new();
        for variant in deletes(&term, max_edit) {
            if let Some(list) = self.by_delete.get(&bk_core::xxh3_str(&variant)) {
                cand.extend_from_slice(list);
            }
        }
        cand.sort_unstable();
        cand.dedup();

        let tlen = term.chars().count();
        let budget = (tlen / 3).clamp(1, max_edit);
        let mut scored: Vec<(String, f32, u32)> = Vec::new();
        for idx in cand {
            let e = match self.entries.get(idx as usize) {
                Some(e) => e,
                None => continue,
            };
            let d = levenshtein(&term, &e.word);
            if d == 0 || d > budget {
                continue;
            }
            let denom = tlen.max(e.word.chars().count()).max(1) as f32;
            let conf = (1.0 - d as f32 / denom).clamp(0.0, 1.0);
            scored.push((e.word.clone(), conf, e.freq));
        }
        scored.sort_unstable_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.2.cmp(&a.2))
        });
        scored.truncate(8);
        scored.into_iter().map(|(w, c, _)| (w, c)).collect()
    }
}

/// Lowercase + keep only alphanumeric characters.
fn clean(w: &str) -> String {
    w.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// All strings reachable by deleting up to `n` characters, including the word.
fn deletes(word: &str, n: usize) -> Vec<String> {
    let chars: Vec<char> = word.chars().collect();
    let mut out = Vec::new();
    let mut seen: rustc_hash::FxHashSet<String> = Default::default();
    out.push(word.to_string());
    seen.insert(word.to_string());
    let mut level = vec![word.to_string()];
    for _ in 0..n {
        let mut next = Vec::new();
        for w in &level {
            let cs: Vec<char> = w.chars().collect();
            for i in 0..cs.len() {
                let mut s = String::with_capacity(cs.len());
                for (j, c) in cs.iter().enumerate() {
                    if j != i {
                        s.push(*c);
                    }
                }
                if seen.insert(s.clone()) {
                    next.push(s);
                }
            }
        }
        out.extend(next.iter().cloned());
        level = next;
        if level.is_empty() {
            break;
        }
    }
    let _ = chars;
    out
}

/// Damerau-Levenshtein (optimal string alignment) distance on chars. Treats an
/// adjacent transposition as a single edit, which catches the most common typos
/// (`ruts` → `rust`).
pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let n = a.len();
    let m = b.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=m {
        d[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            let mut best = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = best;
        }
    }
    d[n][m]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spell() -> SymSpell {
        SymSpell::build(vec![
            ("the".into(), 1000),
            ("their".into(), 500),
            ("there".into(), 400),
            ("example".into(), 300),
            ("bookmark".into(), 200),
        ])
    }

    #[test]
    fn exact_hit() {
        let s = spell();
        assert_eq!(s.lookup("the", 2)[0].0, "the");
    }

    #[test]
    fn corrects_typos() {
        let s = spell();
        let c = s.lookup("exmaple", 2);
        assert_eq!(c.first().map(|x| x.0.as_str()), Some("example"));
        let c = s.lookup("boookmark", 2);
        assert_eq!(c.first().map(|x| x.0.as_str()), Some("bookmark"));
    }

    #[test]
    fn distance() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("abc", "abc"), 0);
    }

    #[test]
    fn bounded_vocab() {
        let words: Vec<_> = (0..30_000).map(|i| (format!("word{i}"), 1)).collect();
        let s = SymSpell::build(words);
        assert!(s.len() <= MAX_WORDS);
    }
}
