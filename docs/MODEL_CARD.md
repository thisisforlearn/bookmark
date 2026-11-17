# Model card

Three small models ship behind one container format. All are optional: without them the server
runs lexical search plus SymSpell.

## 1. Spell model (`spell.bkmdl`)

| | |
|---|---|
| Task | Re-rank candidate corrections for a misspelled query term |
| Input | Character unigrams, bigrams, trigrams of (typo, candidate) hashed into 256 buckets, plus a length-difference feature, L2-normalised |
| Architecture | 256 → 32 (ReLU) → 1 (logistic) |
| Parameters | 256·32 + 32 + 32 + 1 ≈ 8.3 k |
| Training | SGD on synthetic pairs: positives from keyboard/adjacency/transpose/drop/duplicate corruptions of the vocabulary, negatives against unrelated words |
| Data | Built-in seed vocabulary ∪ your bookmarks and tags |
| Objective | Binary cross-entropy on `(typo, candidate)` pairs |
| Size | ~33 KB |
| Evaluation | Training accuracy (reported by the trainer); SymSpell is the fallback |

At query time the score is blended: `0.45 · symspell_confidence + 0.55 · mlp_score`.

## 2. Embedding model (`embed.bkmdl`)

| | |
|---|---|
| Task | Semantic/fuzzy recall for paraphrases and morphological variants |
| Input | Word tokens |
| Method | Hashing vectorizer over the whole word plus character trigrams/4-grams, hash-projected with a sign, then L2-normalised |
| Dimensions | 192 |
| Quantisation | Symmetric int8, one global scale |
| Training | None (deterministic). Improves automatically as the vocabulary grows |
| Size | ~90 KB for 20 k words |
| Evaluation | Similar spellings (e.g. `bookmark` vs `bookmarks`) are measurably closer than unrelated words (unit test) |

This is deliberately a *hashing* embedding, not a transformer: it is deterministic, has no
training cost, and captures sub-word similarity, which is exactly what complements FTS5.
Swapping in a learned encoder later only requires writing the same tensor names.

## 3. LTR re-ranker (`rank.bkmdl`)

| | |
|---|---|
| Task | Order the candidate set for a query |
| Features | fused score, vector similarity, recency, pinned, tag match, domain affinity, has-screenshot, title overlap |
| Architecture | 8 → 8 (GELU) → 1 (logistic) |
| Default | Identity first layer + tuned heuristic weights, so it reproduces the hand-tuned ranking |
| Size | ~0.5 KB |
| Evaluation | nDCG is the goal metric; `clicks` records the labels needed to train it properly later |

## Container format

```
"BKMDL1" | version:u16 | kind:u8 | arch:u32 | tensor_count:u32 | vocab_len:u32 | flags:u32
section*  : name:lstr | rank:u8 | ndims:u32 | dims:u32* | dtype:u8 | scale:f32 | zero:i8
            | offset:u64 | len:u64 | crc32c:u32
vocab*    : token:lstr
payload*  : raw tensor bytes
trailer   : xxh3:u64
```

Validation is strict: magic, version, section bounds, per-tensor CRC32C, quantisation range.
Any failure returns `Error::Model`; the server then runs without that model and shows the
**degraded** badge.

## Retraining

```sh
bookmark model train spell --mode full
bookmark model train embed
bookmark model verify
```

Restart the server to load new files. Models are evaluated for shape and CRC on load; a
mismatched or truncated file is rejected rather than silently producing garbage.
