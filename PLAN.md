# Bookmark Server — Implementation Plan (v1)

A self-hostable, multi-user bookmark server that stores **text bookmarks, screenshots and
URLs**, crawls each URL into a **lightweight searchable index**, and uses **small models we
train ourselves** for typo-tolerant search, semantic recall and result re-ranking.
Runs happily on **Termux/Android (aarch64)** from a single static binary.

---

## 1. Decisions locked in from the interview

| Topic | Decision |
|---|---|
| Language | **Rust** for the entire project (server, crawler, search, inference, training tool). Rust 2021/2024, GNU toolchain (`x86_64-unknown-linux-gnu` on this box, `x86_64-pc-windows-msvc` when you train on Windows 11). |
| License | **GPL-3.0-only**, © 2026 Vaibhav. No MIT, no Apache, no AGPL. Dependency licenses enforced with `cargo deny` (GPL-3.0-only must be compatible with every crate we link). |
| Screenshots | **Client uploads** (browser/PWA/extension/bookmarklet POSTs PNG/JPEG; server validates, dedupes, stores in SQLite blobs). No headless browser on device. |
| Crawler | **Light**: single page, readability extraction, no JS, no deep crawl, robots.txt respected, strict timeouts/size caps, SSRF guard. |
| Models | Three small models, trained by us: **(1) query spell correction, (2) semantic embeddings, (3) LTR re-ranker**. All exported as one portable **flat int8 `.bkmdl`** container. |
| Training | **Burn 0.22 + `wgpu` backend** (Vulkan/DX12/Metal) so it uses the Intel Iris Xe on Windows 11, with `--backend cpu` fallback (CubeCL CPU) for WSL2 debugging. `indicatif` progress bar + ETA + resumable checkpoints. Target ≤ 2 h end-to-end, `--quick` ≤ 25 min, `--smoke` ≤ 5 min. |
| Runtime inference | **Rust `std::arch` kernels**: AVX2 + **AVX-VNNI** (`dpwssd`) on x86_64 (verified present on this i5-1335U), **NEON `sdot`** on aarch64 Termux, scalar reference + differential tests. No C++, no ONNX Runtime, ~0.5–2 MB model, mmap + zero-copy. |
| Accounts | **Multi-user**: Argon2id passwords, DB-backed sessions, per-user library, optional share links. |
| Web UI | Server-rendered **Askama + htmx** + ~5 KB of vanilla JS. No SPA, no node build step. |
| Storage | **SQLite (bundled) + FTS5** (unicode61/porter + trigram + LIKE for 1–2 char queries). Int8 vectors in a blob table; brute-force AVX2 search (fine to ~200k docs). |
| Distribution | Prebuilt **static aarch64 musl** binary for Termux + `curl … | bash` installer, `bookmarkctl`-style CLI, watchdog, `termux-boot`, systemd unit, Termux notifications, backup/export. |
| GitHub | Create **public** repo `thisisforlearn/bookmark`, commit author **Vaibhav**, push `main`. |

> **Language note (important):** you first picked "C++20 server + C++ AVX2/NEON inference",
> then chose **Rust for the whole project**. Because the whole project is Rust, the flat
> int8 runtime is implemented in **Rust with `std::arch` intrinsics** (AVX2/AVX-VNNI/NEON),
> not C++. Same design, same single-binary deployment, same SIMD paths — different language.
> If you actually want a C ABI static lib for the inference core, say so and I will switch
> that one crate back to C++20.

---

## 2. Repository layout

```
bookmark/
├── Cargo.toml                    # workspace + release profile (lto, codegen-units=1, strip, panic=abort)
├── Cargo.lock
├── deny.toml                     # GPL-3.0-only license gate
├── rust-toolchain.toml           # stable, rustfmt, clippy
├── LICENSE                       # GPL-3.0-only
├── README.md                     # super-easy: quickstart, Termux, Tailscale, LAN, systemd, models
├── install.sh                    # curl | bash installer (arch detect, sha256 verify, idempotent)
├── .github/workflows/ci.yml      # fmt, clippy -D warnings, test, cargo deny, matrix build
├── .github/workflows/release.yml # static musl + android + windows-train builds, sha256, release
├── crates/
│   ├── bk-core/      # ids, errors, config, time, hashing (xxh3), result types, feature flags
│   ├── bk-model/     # .bkmdl container: header, sections, CRC, mmap loader, model spec, quantize
│   ├── bk-infer/     # AVX2/AVX-VNNI + NEON int8 GEMM, char CNN, bi-encoder, MLP ranker, spell scoring
│   ├── bk-db/        # schema + migrations, pool, writer task, FTS sync triggers, blob store
│   ├── bk-crawl/     # fetch (reqwest/rustls), robots, SSRF guard, charset, readability, lang, dedupe
│   ├── bk-search/    # query parse, SymSpell, FTS5 + trigram + LIKE, vector search, RRF fusion, LTR
│   ├── bk-serve/     # axum router, auth, htmx/Askama pages, uploads, share links, static embed
│   ├── bk-train/     # Burn training binaries: train-spell, train-embed, train-rank, export-model
│   ├── bk-cli/       # `bookmark` binary: init/user/serve/watch/crawl/index/search/add/export/doctor/model
│   └── bk-ops/       # supervisor (watchdog), termux-boot, systemd unit, notifications, backup
├── migrations/       # 0001_init.sql … (embedded, applied on boot, forward-only)
├── models/           # shipped default .bkmdl (small) + train configs (json) + eval reports
├── benches/          # divan: search p50/p95, int8 GEMM, crawl throughput, page render
├── tests/            # integration (axum oneshot), crawl fixtures, fuzz corpora
└── docs/             # ARCHITECTURE.md, MODEL_CARD.md, TERMUX.md, TAILSCALE.md, TROUBLESHOOTING.md
```

Build flags: `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`,
`opt-level = 3`. `mimalloc` as global allocator (tuned for low RSS). Dev builds use
`-C target-cpu=native`; release builds stay portable with **runtime** feature detection.

---

## 3. Data model (SQLite, WAL)

```
users(id, username UNIQUE, pw_hash, role, created_at, settings_json)
sessions(id, user_id, token_hash, created_at, expires_at, ip, ua)
bookmarks(id, user_id, url_norm UNIQUE(user_id,url_norm), url, title, description, text,
          text_hash, lang, site, status, note, pinned, created_at, updated_at, crawled_at)
tags(id, user_id, name, color)                bookmark_tags(bookmark_id, tag_id)
screenshots(bookmark_id PK, mime, width, height, bytes, sha256, blob)
crawls(id, bookmark_id, http_status, error_kind, attempts, next_try_at, bytes, ms)
vectors(bookmark_id, model_id, dim, quant, blob)        -- int8 + per-row scale
clicks(id, user_id, query, bookmark_id, rank, ts)       -- LTR labels
shares(id, user_id, bookmark_id, token, expires_at)
fts_title/body/bookmark_fts   (contentless/external-content, triggers keep in sync)
fts_tri                       (trigram tokenizer for substring + 1–2 char LIKE)
meta(key, value)              (schema_version, model_ids, last_index_ts)
```

Indexes: `(user_id, created_at DESC)`, `(user_id, site)`, `(user_id, url_norm)`,
`vectors(model_id)`, `clicks(user_id, ts)`. `PRAGMA journal_mode=WAL`, `synchronous=NORMAL`,
`busy_timeout=5000`, `mmap_size=256MB`, `cache_size=-32000`, `foreign_keys=ON`.

Writes go through **one writer task** fed by an mpsc channel and batched in transactions
(≤ 25 ms window or 512 rows) — avoids SQLITE_BUSY storms from many htmx requests.

---

## 4. HTTP surface (axum 0.8, tokio)

Public: `GET /login`, `POST /login`, `POST /register` (if open), `GET /healthz`, `GET /share/:token`.
Authed: `GET /` (htmx stream: search + list), `POST /bookmarks` (URL/text/screenshot multipart),
`POST /bookmarks/:id/tags`, `POST /bookmarks/:id/pin`, `DELETE /bookmarks/:id`,
`GET /b/:id` (detail + screenshot), `GET /screenshot/:id` (ETag, immutable),
`GET /search?q=` (HTML + `Accept: application/json`), `POST /import`, `GET /export`,
`POST /crawl/:id/retry`, `GET /settings`, `GET /api/v1/*` (JSON mirror for the CLI/PWA).
 hardening: per-user rate limit, `Content-Security-Policy`, `X-Content-Type-Options`,
`X-Frame-Options: DENY`, CSRF double-submit on all mutations, upload caps (text 1 MB,
screenshot 5 MB), image sniffing via `image` crate, no path from user input to filesystem.

---

## 5. Crawler pipeline (bk-crawl)

1. `url_norm` (lowercase host, strip `utm_*`, fragment, default ports, IDNA via `idna`).
2. **SSRF guard**: resolve, reject private/loopback/link-local/CGNAT/multicast, pin resolved IP,
   re-check on every redirect (≤ 3), block non-http(s).
3. Fetch: `reqwest` + rustls, h2, brotli/gzip/zstd, 10 s connect / 15 s total, 2 MB body cap,
   1 request per host per 1.5 s (polite), global semaphore = 4 (1 on battery-saver).
4. Charset detect (`encoding_rs`), `lol_html`/`scraper` strip script/style/nav, readability
   scoring (text density + link density + heading signals) → title, byline, date, main text,
   code blocks preserved, max 400 KB text stored.
5. `whatlang` language, xxh3 dedupe hash, boilerplate ratio check (reject if < 8% text).
6. Upsert bookmark + FTS + vectors; `crawls` row records status/error for retry with
   exponential backoff (429/5xx/timeouts). Never panics on malformed HTML (html5ever tolerant).
7. Optional: `bookmark crawl --all --limit N` batch job, resumable, progress bar.

Failure matrix is explicit: DNS fail, TLS fail, 403/429, timeout, too large, empty body,
charset unknown, non-HTML (PDF/image → store URL + mime only). Each is a typed `CrawlError`
with a user-visible message and a retry policy.

---

## 6. Search pipeline (bk-search)

```
raw query
 ├─ parse: "quoted", tag:foo, site:example.com, is:screenshot, before:2024-01-01, -term
 ├─ normalize (NFKC, casefold, strip diacritics, split CJK chars)
 ├─ candidate generation: SymSpell deletes (edit ≤ 2) over per-user vocab + global vocab
 ├─ spell scoring: tiny char-CNN/MLP noisy-channel model → top-1 correction + confidence
 ├─ lexical: FTS5 (porter/unicode61) + trigram for substrings; LIKE for 1–2 char tokens
 ├─ semantic: bi-encoder query vector → int8 dot over vectors (AVX2/VNNI or NEON)
 ├─ fuse: weighted RRF (lexical 0.6 / semantic 0.4, adaptive if one arm is empty)
 ├─ rerank: LTR MLP over top 100 (bm25, vec sim, recency, tag match, domain affinity, CTR)
 └─ return top 50 + snippet highlight + "did you mean" + timing breakdown
```

Every stage is optional and guarded: if the model file is missing/corrupt, or SIMD feature is
absent, or FTS is unavailable, the server degrades to plain FTS5 + SymSpell and shows a small
"degraded" badge in the UI. Query results always include `corrected_query`, `took_ms`,
`degraded: bool`.

---

## 7. Models (bk-train) — what we train, how, and the 2 h budget

All three models are tiny and share one export format. Training uses Burn 0.22 (`wgpu`
backend on Windows 11 → DX12/Vulkan on the Iris Xe; `cpu` on WSL2). One dispatch queue
(Intel iGPU driver crashes with ≥3 concurrent dispatches — known issue), pinned device,
`indicatif` multi-progress bar with ETA, checkpoint every N steps, `--resume`.

### 7.1 Spell model (char-CNN/MLP noisy channel, ~0.8–1.5 M params)
- Input: typo + candidate pair encoded as char n-gram features + edit-op features.
- Labels: synthetic corruptions of real vocabulary (keyboard adjacency incl. QWERTY/phone,
  OCR confusions, phonetic swaps, drop/transpose/duplicate, camelCase splits, URL-ish noise).
- Loss: pairwise ranking (correct candidate must outrank others) + LM prior term.
- Data: your corpus (titles, tags, content top-k sentences) + optional word list + templates.
- Eval: top-1 accuracy / MRR on held-out synthetic typos, CER/WER; must beat SymSpell baseline.

### 7.2 Embedding model (mini bi-encoder, 4 layers, d=256, ~8–12 M params)
- Char/wordpiece vocab built from corpus (≤ 32 k).
- Positives: title ↔ best content sentence, tag ↔ title, URL slug ↔ title, question ↔ answer-ish
  sentence (template-generated). Hard negatives: in-batch + BM25 near-misses.
- Loss: InfoNCE with temperature, 2 s/query for the crawl.
- Eval: nDCG@10 / recall@50 on a held-out split of your bookmarks + a synthetic paraphrase set.

### 7.3 LTR re-ranker (MLP, 64 units × 2 layers, ~20 k params)
- Features: bm25, vector cos, recency, pinned, tag match, domain affinity, screenshot present,
  historical CTR per (query-token, doc) bucket.
- Labels: clicks + synthetic relevance from BM25/vector agreement; pairwise logistic (RankNet).
- Eval: nDCG@10 vs BM25-only, click-through proxy.

### 7.4 Export: `.bkmdl` flat int8 container
```
magic "BKMDL1" | version | model kind | arch flags | tensor count
sections: [name][rank][dims][dtype=int8/fp16][scale][zero][offset][crc32c]
vocab: [token][score] ... | thresholds | feature order | trained_on | checksum(xxhash3)
```
mmap, page-aligned, zero-copy, validated (magic, version, size, per-section CRC, quant range).
Quantization: per-tensor symmetric int8 with calibration on 512 held-out samples; fp16
fallback for the embedding output projection. Sizes: spell < 4 MB, embed ~10 MB fp16 / ~3 MB
int8, rank < 100 KB. Total default bundle < 8 MB, shipped in `models/`.

### 7.5 Training UX (this is what you asked for)
```
$ bookmark train spell --backend wgpu --steps 40000 --quick
  ▸ data      building corpus … 128,441 sentences (2.1 s)
  ▸ device    Intel(R) Iris(R) Xe Graphics  [dx12]  autodiff
  ███████████████████░░░░░░░  34,120/40,000  step 12.4k/s  loss 0.412  val@1 0.931  ETA 00:08
  ✔ exported models/bookmark-spell.bkmdl  (1.9 MB, int8, crc ok)
```
Modes: `--smoke` (5 min, sanity), `--quick` (25 min, usable), `--full` (≤ 2 h, default target),
`--steps/--hours` override. Crash-safe resume from `target/train/<name>/checkpoint`.
A `--dry-run` prints the full step/time estimate before touching the GPU.

### 7.6 Runtime inference (bk-infer)
- Kernels: int8 GEMM (AVX-VNNI `dpwssd`, AVX2 `maddubs+madd` fallback), char embedding lookup
  (gather + AVX2), LayerNorm/RMSNorm, GELU/ReLU, softmax (AVX2 exp), max-pool, NEON `sdot` path.
- Runtime dispatch via `is_x86_feature_detected!` / `is_aarch64_feature_detected!`; scalar
  reference for tests; differential tests (SIMD vs scalar, tolerance ≤ 1 LSB after quant).
- Hot model kept in `ArcSwap`, no locks on the query path; per-request buffers from a
  thread-local pool (no allocation in steady state).

---

## 8. Performance targets (measured, not aspirational)

| Metric | Target (this i5-1335U / Termux aarch64) |
|---|---|
| Search p50 / p95 (10 k bookmarks) | ≤ 6 ms / ≤ 25 ms (Termux ≤ 20 / 60 ms) |
| Spell correction per query | ≤ 0.8 ms (AVX2) / ≤ 2 ms (NEON) |
| Semantic arm (≤ 50 k docs) | ≤ 4 ms int8 AVX2 |
| Page render (htmx) | ≤ 3 ms p95, HTML ≤ 40 KB |
| Idle RSS / with 50 k docs | ≤ 45 MB / ≤ 180 MB |
| Binary size (static musl, stripped) | ≤ 14 MB |
| Cold start to ready | ≤ 350 ms (model mmap) |
| Crawl throughput (WiFi, 4 workers) | 25–60 pages/s, 0 dropped on backpressure |

Techniques: single writer task + batched transactions, prepared-statement cache, `rust-embed`
pre-compressed assets, brotli for HTML, `ArcSwap` model, thread-local scratch, `smallvec`,
`rustc-hash`, `aho-corasick` for tag/domain matching, `memchr` scanning, `rayon` for index
backfill only (never on request path), mimalloc + `MALLOC_ARENA_MAX` tuning.

---

## 9. Termux + ops (bk-ops, install.sh)

`install.sh` (works with `curl … | bash`):
- detects `aarch64`/`armv7`/`x86_64` Android, downloads the matching static binary +
  sha256, verifies, installs to `$PREFIX/bin/bookmark` (or `~/.local/bin`), `chmod +x`,
  creates `$PREFIX/var/lib/bookmark` + `~/.config/bookmark/config.toml`, runs
  `bookmark init`, prints the URL + token, and offers (interactive prompt, skippable with
  `--yes`) to install the boot script and to `termux-wake-lock`.
- `--uninstall`, `--version`, `--dev` (install from a local build), fully idempotent.

CLI (`bookmark`):
`init` · `serve [--host 0.0.0.0 --port 8080]` · `watch` (supervisor) · `user add/list/rm` ·
`add <url|text>` · `search <q>` · `crawl [--all]` · `index --rebuild` · `export/import` ·
`backup` · `model list/train/export/verify` · `doctor` (checks: db, model, ports, tailscale,
disk, perms, SIMD features) · `service install|start|stop|status|logs` · `update`.

Restart machinery:
- **`bookmark watch`**: supervisor loop, restarts the server on exit with exponential backoff
  (1 s → 30 s), health-checks `/healthz` every 10 s, sends a Termux notification on ≥3 crashes,
  optional `--max-restarts`.
- **Termux**: `~/.termux/boot/start-bookmark.sh` (starts `termux-wake-lock` + `bookmark watch`),
  optional `termux-job-scheduler` entry instead of a raw loop, `termux-notification` on crash.
- **Linux/WSL**: `bookmark.service` (systemd, `Restart=always`, `RestartSec=2`, `LimitNOFILE`),
  logrotate snippet, `bookmark service install` writes + enables it.
- **Windows (dev)**: `bookmark service install` registers a Scheduled Task at logon.
- Keeps a 32 MB ring log at `$STATE/bookmark.log` with rotation.

Backup/export: JSON export (bookmarks + tags + notes, screenshots as base64 optional),
`bookmark backup` → tar.zst snapshot with sha256 manifest, `import` validates and is idempotent
by `url_norm`. Cron/`termux-job-scheduler` example in the README.

---

## 10. Security

Argon2id (OWASP params) · constant-time token compare · DB-backed sessions with rotation ·
CSRF double-submit on every mutation · CSP `default-src 'self'` (no inline scripts except a
nonce) · SSRF guard (DNS pin, private-range block, redirect re-validation) · upload sniffing +
size caps · per-user rate limit + global concurrency cap · `cargo audit` + `cargo deny` in CI ·
no secrets in the repo (generated token printed once) · `file`-permission 0700 on state dir ·
optional `--require-tailscale` mode (only accept connections from the tailnet interface).

---

## 11. Testing & quality gates

- Unit: url normalize, charset, readability fixtures, SymSpell (vs brute force), query parser,
  quantize/dequantize round-trip, `.bkmdl` corruption cases (truncated, bad CRC, bad version).
- SIMD: differential tests AVX2/VNNI/NEON vs scalar; `#[ignore]` perf sanity test with tolerance.
- Integration: axum `oneshot` for every route; multi-user isolation; CSRF; upload rejection;
  crawl against a local fixture server (mock 200/403/429/timeout/huge/binary/charset).
- Fuzz (cargo-fuzz): HTML extraction, `.bkmdl` parser, query parser, multipart parser.
- Property (proptest): normalizers, SymSpell candidate generation, quantize round-trip.
- Perf: divan benches in CI (trend-comment), plus `docs/PERF.md` with numbers.
- Gates: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --all`,
  `cargo deny check licenses`, `cargo audit`.
- Model gates: spell top-1 ≥ baseline + 8 %, embed nDCG@10 ≥ BM25, LTR nDCG@10 ≥ BM25; fail
  the release if a model regresses (report committed in `models/eval/`).

---

## 12. GitHub / release

1. `git init`, local `user.name=Vaibhav`, `user.email=vaibhav@users.noreply.github.com` (change on request).
2. `gh repo create thisisforlearn/bookmark --public --source . --push` (main branch).
3. Commits: conventional-ish messages, every file with
   `SPDX-License-Identifier: GPL-3.0-only` + `Copyright (C) 2026 Vaibhav`.
4. CI: fmt, clippy, test, deny, audit; matrix release build for
   `aarch64-unknown-linux-musl` (Termux), `x86_64-unknown-linux-musl`, `aarch64-linux-android`
   (NDK, optional), `x86_64-pc-windows-msvc` (train tool), attach `install.sh`, sha256 sums,
   `models/*.bkmdl`.
5. README (easy to read, with a TOC): what it is → 3-step quickstart → Termux install →
   starting/stopping/restarting → LAN access → **Tailscale** (`tailscale up`, `tailscale serve`,
   HTTPS, `--require-tailscale`) → systemd → models & training (progress bar) → backup →
   screenshots → troubleshooting → security → license.

---

## 13. Phases (each phase ends with `cargo test` + a demo)

- **P0 Scaffold (0.5 day)**: workspace, license headers, CI, deny, fmt/clippy, skeleton crates,
  `bookmark --version`, README skeleton, install.sh (local `--dev`), git init + first push.
- **P1 Core (1 day)**: schema + migrations, user/auth/session, single-user CRUD, Askama+htmx
  list/detail, text bookmarks, `bookmark add/search`, `/healthz`.
- **P2 Crawl + index (1 day)**: fetch pipeline, SSRF guard, robots, charset, readability, FTS5
  sync, crawl status/retry, `bookmark crawl --all` with progress bar.
- **P3 Screenshots + multi-user + share (0.5 day)**: upload/dedupe/resize/thumbnail, per-user
  isolation, share links, import/export, backup.
- **P4 Search v1 (0.5 day)**: query parser, SymSpell, FTS5 + trigram + LIKE, fusion, snippets,
  benchmarks.
- **P5 Models (2 days)**: `.bkmdl` + kernels + SymSpell baseline; spell training + export;
  embed training + vector arm; LTR training + rerank; eval reports; graceful degradation.
- **P6 Ops (0.5 day)**: watchdog, termux-boot, systemd, notifications, doctor, update, README
  rewrite, Termux smoke test script, release CI + first tagged release.

**Total ≈ 6 working days of agent time.** You can stop after any phase and still have a
working, useful server; P1 alone is a usable text-bookmark server, P2 adds search over page
content, P5 adds the models.

---

## 14. Risks & mitigations

| Risk | Mitigation |
|---|---|
| wgpu on Intel iGPU (DX12) instability | serialize dispatches, `--backend cpu` fallback, pin Burn/wgpu versions, smoke-test 10× before training |
| WSL2 has no `/dev/dri` | training is CPU/DX12 there; real training on your Windows 11 box |
| Termux kills the process | wake-lock + boot script + watchdog + notification |
| SQLite writer contention | single writer task + WAL + busy_timeout + batching |
| 1–2 char queries unsupported by FTS5 trigram | LIKE fast-path over trigram table (documented, benchmarked) |
| Model hurts ranking | shipped behind flags, A/B via `clicks`, automatic fallback to BM25 |
| GPL + dependency licenses | `cargo deny` gate in CI; no AGPL/SSPL deps |
| Scope creep | phases are shippable; defaults chosen so the server works with zero model files |

---

## 15. Definition of done (v1.0)

1. `curl … | bash` on Termux installs, starts, and serves the UI on `127.0.0.1:8080`; it survives
   a phone unlock, a crash, and a reboot (boot script), and notifies on repeated crashes.
2. Text bookmarks with spell-corrected search; URL bookmarks crawled into searchable text;
   screenshots uploadable, thumbnailed, deduped.
3. Multi-user accounts, per-user isolation, share links, export/import/backup.
4. All three models train via `bookmark train …` with a progress bar, ≤ 2 h on the Iris Xe,
   and are exported as one `.bkmdl` that the server loads mmap-zero-copy with AVX2/NEON kernels.
5. `cargo fmt/clippy/test` green, `cargo deny` green, CI builds all release targets, public repo
   with a README a first-time self-hoster can follow end-to-end (local → LAN → Tailscale).
6. GPL-3.0-only, © 2026 Vaibhav, on `main` at github.com/thisisforlearn/bookmark.

---

## 16. Open questions (defaults already chosen, tell me to change any)

1. Git author email: default `vaibhav@users.noreply.github.com`.
2. Open registration or invite-only? Default: **invite-only** (admin creates users).
3. Tailscale-only mode default? Default: bind loopback, `--host` for LAN/Tailscale.
4. Keep default models in git (binary, ~8 MB) or Git LFS? Default: **in git**, no LFS.
5. Mobile UI: responsive HTML (default) or add a PWA manifest + install prompt (yes, cheap).
6. Browser extension for one-click "save page + screenshot"? Default: **bookmarklet first**,
   extension only if you ask.