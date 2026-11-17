// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Corpus building: seed vocabulary + the user's own bookmarks, plus a synthetic
//! typo generator (keyboard adjacency, transpose, drop, duplicate, substitute).

use std::collections::HashMap;

use rand::Rng;

use bk_core::Id;
use bk_db::Db;

/// Seed vocabulary merged with words from the user's bookmarks and tags.
pub fn build_vocabulary(db: &Db, user: Option<Id>, limit: usize) -> Vec<(String, u32)> {
    let mut freq: HashMap<String, u32> = HashMap::new();
    for (w, f) in bk_search::seed_words::seed_words() {
        *freq.entry(w.to_string()).or_insert(0) += f;
    }
    let _ = db.with_conn_sync(|conn| {
        let sql = match user {
            Some(_) => "SELECT title, text FROM bookmarks WHERE user_id = ?",
            None => "SELECT title, text FROM bookmarks",
        };
        let mut stmt = conn.prepare(sql)?;
        let mut collect = |title: String, text: String| {
            add_words(&mut freq, &title, 6);
            let tail: String = text.chars().take(4000).collect();
            add_words(&mut freq, &tail, 1);
        };
        if let Some(u) = user {
            let rows = stmt.query_map([u.inner()], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for (t, x) in rows.flatten() {
                collect(t, x);
            }
        } else {
            let rows =
                stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for (t, x) in rows.flatten() {
                collect(t, x);
            }
        }
        let mut tstmt = conn.prepare("SELECT name FROM tags")?;
        let trows = tstmt.query_map([], |r| r.get::<_, String>(0))?;
        for t in trows.flatten() {
            *freq.entry(t.to_lowercase()).or_insert(0) += 10;
        }
        Ok(())
    });
    let mut v: Vec<(String, u32)> = freq.into_iter().collect();
    v.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    v.truncate(limit);
    v
}

fn add_words(map: &mut HashMap<String, u32>, text: &str, weight: u32) {
    let mut cur = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            cur.extend(ch.to_lowercase());
        } else if !cur.is_empty() {
            if cur.len() >= 3 && cur.len() <= 30 {
                *map.entry(std::mem::take(&mut cur)).or_insert(0) += weight;
            } else {
                cur.clear();
            }
        }
    }
}

/// Neighbouring keys on a QWERTY keyboard, for realistic substitutions.
fn neighbours(c: char) -> &'static [char] {
    match c {
        'a' => &['s', 'q', 'z'],
        'b' => &['v', 'g', 'h', 'n'],
        'c' => &['x', 'd', 'f', 'v'],
        'd' => &['s', 'e', 'r', 'f', 'c', 'x'],
        'e' => &['w', 'r', 's', 'd'],
        'f' => &['d', 'r', 't', 'g', 'v', 'c'],
        'g' => &['f', 't', 'y', 'h', 'b', 'v'],
        'h' => &['g', 'y', 'u', 'j', 'n', 'b'],
        'i' => &['u', 'o', 'j', 'k'],
        'j' => &['h', 'u', 'i', 'k', 'm', 'n'],
        'k' => &['j', 'i', 'o', 'l', 'm'],
        'l' => &['k', 'o', 'p'],
        'm' => &['n', 'j', 'k'],
        'n' => &['b', 'h', 'j', 'm'],
        'o' => &['i', 'p', 'k', 'l'],
        'p' => &['o', 'l'],
        'q' => &['w', 'a'],
        'r' => &['e', 't', 'd', 'f'],
        's' => &['a', 'w', 'e', 'd', 'x', 'z'],
        't' => &['r', 'y', 'f', 'g'],
        'u' => &['y', 'i', 'h', 'j'],
        'v' => &['c', 'f', 'g', 'b'],
        'w' => &['q', 'e', 'a', 's'],
        'x' => &['z', 's', 'd', 'c'],
        'y' => &['t', 'u', 'g', 'h'],
        'z' => &['a', 's', 'x'],
        _ => &[],
    }
}

/// Produce a single realistic corruption of `word`.
pub fn corrupt(word: &str, rng: &mut impl Rng) -> String {
    let chars: Vec<char> = word.chars().collect();
    if chars.len() < 3 {
        return word.to_string();
    }
    let op = rng.random_range(0..5);
    let mut out = chars.clone();
    match op {
        0 => {
            // drop
            let i = rng.random_range(0..out.len());
            out.remove(i);
        }
        1 => {
            // transpose
            let i = rng.random_range(0..out.len() - 1);
            out.swap(i, i + 1);
        }
        2 => {
            // duplicate
            let i = rng.random_range(0..out.len());
            let c = out[i];
            out.insert(i, c);
        }
        3 => {
            // substitute with a keyboard neighbour
            let i = rng.random_range(0..out.len());
            let cands = neighbours(out[i]);
            if !cands.is_empty() {
                out[i] = cands[rng.random_range(0..cands.len())];
            }
        }
        _ => {
            // insert a random letter
            let i = rng.random_range(0..=out.len());
            let letters = b"abcdefghijklmnopqrstuvwxyz";
            out.insert(i, letters[rng.random_range(0..26)] as char);
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corruption_changes_word() {
        let mut rng = rand::rng();
        let mut changed = 0;
        for _ in 0..50 {
            if corrupt("bookmark", &mut rng) != "bookmark" {
                changed += 1;
            }
        }
        assert!(changed > 40);
    }
}
