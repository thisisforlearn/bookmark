# Architecture

```
                       ┌─────────────┐
   browser / curl ───▶ │  bk-serve   │  axum 0.8, Askama templates, ~4 KB JS
                       └──────┬──────┘
              ┌───────────────┼────────────────┐
              ▼               ▼                ▼
        ┌──────────┐   ┌────────────┐   ┌────────────┐
        │ bk-search│   │   bk-db    │   │ bk-crawl   │
        └────┬─────┘   └─────┬──────┘   └─────┬──────┘
             │               │                │
             ▼               ▼                ▼
        ┌──────────┐   ┌────────────┐   ┌────────────┐
        │ bk-infer │   │   SQLite   │   │ remote web │
        └────┬─────┘   │  +  FTS5   │   └────────────┘
             ▼         └────────────┘
        ┌──────────┐
        │ bk-model │  .bkmdl (mmap, CRC32C)
        └──────────┘
```

## Request path

1. **Middleware** (`bk-serve/src/security.rs`): rate limit → CSRF token issue → security headers.
2. **Auth** (`bk-serve/src/auth.rs`): the `AuthUser` extractor reads the `bk_session` cookie,
   hashes it, and looks it up in `sessions`. Unauthenticated requests to HTML routes are
   redirected to `/login`; API routes get `401`.
3. **Handler** builds a `SearchRequest` and runs `Searcher::search` on a blocking thread.

## Storage

SQLite in WAL mode. One **writer task** owns the write connection; all mutations are sent over
an mpsc channel and batched into transactions (flush at 25 ms or 512 jobs). Readers use a small
pool of connections, so a long crawl never blocks a search.

`bookmarks` is the central table. FTS5 tables (`bookmark_fts`, `fts_tri`) are **external
content** tables kept in sync by `AFTER INSERT/UPDATE/DELETE` triggers — there is no separate
reindex step in the write path.

`vectors` stores int8 embeddings with a per-row scale. `clicks` is the LTR label table.

## Search pipeline

```
raw query
  ├─ parse            "phrases", tag:, site:, is:screenshot, before:/after:, -negated
  ├─ spell            SymSpell deletes ≤2 (+ trained MLP blend) → corrected query
  ├─ lexical          FTS5 AND → OR → trigram → LIKE(1–2 chars)
  ├─ semantic         bi-encoder query vec → int8 dot over vectors
  ├─ fuse             weighted RRF (lexical 0.6 / semantic 0.4, k=60)
  ├─ filter           tags, sites, dates, screenshots, negation
  ├─ re-rank          LTR model or heuristic over 8 features
  └─ snippet          HTML-escaped window with <mark>
```

Every stage is optional. If a model file is missing, its arm is skipped and `degraded: true`
is returned so the UI can say so.

## Crawler

`url_norm` (via the `url` crate, which handles IDNA) is the dedupe key. The SSRF guard resolves
the host with `to_socket_addrs`, rejects non-public addresses, and the redirect policy re-checks
every hop. Bodies are streamed with a 2 MB cap. HTML is stripped of `script/style/nav/header/
footer/aside/form` with `lol_html`, then parsed by `scraper`; the densest `article/main`
paragraph container wins. Text is capped at 400 KB and hashed with xxh3 for dedupe.

## Inference

`bk-infer` implements int8 GEMM with three paths: scalar (the test oracle), AVX2
(`cvtepi8_epi16` + `madd_epi16`), and a NEON path on aarch64. Dispatch is
`is_x86_feature_detected!` at runtime, so one binary runs everywhere.

Models are loaded with `mmap` and validated up front: magic, version, section bounds, per-tensor
CRC32C. A corrupt file is a typed `Err`, never a panic.

## Crate map

| Crate | Responsibility |
|---|---|
| `bk-core` | errors, ids, time, hashing, config, SIMD detection |
| `bk-model` | `.bkmdl` reader/writer, CRC32C, validation |
| `bk-infer` | kernels + spell/embed/rank forward passes |
| `bk-db` | schema, migrations, writer task, CRUD, FTS |
| `bk-crawl` | URL normalization, SSRF, robots, fetch, readability |
| `bk-search` | query parse, SymSpell, fusion, re-rank, snippets |
| `bk-serve` | axum router, auth, handlers, templates, assets |
| `bk-ops` | backup, doctor, service files, paths |
| `bk-cli` | the `bookmark` binary and all subcommands |
| `bk-train` | pure-Rust trainers + `.bkmdl` export (optional feature) |

## Performance notes

- Prepared statements are reused per connection; the read pool avoids reopening SQLite.
- The search vocabulary is cached per user in a `DashMap` and invalidated on write.
- The crawler uses a global semaphore plus a per-host politeness map (`DashMap`).
- Release profile: `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`.
