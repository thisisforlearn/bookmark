# Implementation Plan — `bookmark` v1 (execution view)

Derived from `PLAN.md` (approved design, treated as source of truth). This document is the
**build order**: file-level tasks, shared contracts, and a done-when check for each step.
`PLAN.md` is not modified.

Task IDs are `P<n>-<k>`. Every task lists the files it creates/touches, the deliverable, and
the check that proves it is done.

---

## 0. Ground rules (apply to every task)

| Rule | Detail |
|---|---|
| Language | Rust only in the shipped tree. No C/C++, no Python, no Node build step. |
| License | Every file starts with `SPDX-License-Identifier: GPL-3.0-only` + `Copyright (C) 2026 Vaibhav`. Comment style per file type (`//`, `#`, `--`). |
| Errors | `bk_core::Result<T>` everywhere; `thiserror` enums; **no `unwrap()`/`expect()`/`panic!()` on any request path** (clippy `unwrap_used` is not enabled globally, but review blocks it). |
| SQL | `rusqlite` params only, never string-concatenated SQL. |
| Async | tokio. No blocking calls inside async fns (SQLite/crawl-body-size work goes through `spawn_blocking` or the writer task). |
| New deps | Only from the matrix in §2, or the matrix row is updated in the same commit. |
| Commits | Conventional-ish (`feat:`, `fix:`, `chore:`, `test:`, `docs:`). Author `Vaibhav <vaibhav@users.noreply.github.com>`, set **repo-local** only. |
| Phase gate | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --all` must pass, plus the phase's demo command. |

Header snippet:

```rust
// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav
```

---

## 1. Build configuration (P0, fixed up front)

`Cargo.toml` (workspace root):

```toml
[workspace]
resolver = "3"
members = ["crates/*"]
default-members = ["crates/bk-cli"]          # `cargo build` must NOT compile Burn

[workspace.package]
edition = "2024"
rust-version = "1.97"
license = "GPL-3.0-only"

[profile.release]
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
opt-level = 3

[profile.dev.package."*"]
opt-level = 2                                # keep tests fast without a debug build of deps
```

`rust-toolchain.toml`: `channel = "1.97"`, `components = ["rustfmt", "clippy"]`.

`bk-cli` features:

```toml
[features]
default = []
train  = ["dep:bk-train"]                    # Windows train binary only
```

Rationale: `bk-train` (Burn + wgpu) is ~1 GB of deps and cannot build for musl/Android. The
Termux/musl release binary is built with `--no-default-features`; the Windows training binary
is built with `--features train`. `default-members` keeps `cargo build` on the server path.

---

## 2. Dependency matrix (pinned at P0; versions verified against crates.io 2026-11)

All permissive → GPL-3.0-only compatible. `cargo deny` allowlist is in §2.1.

### 2.1 `deny.toml`

```toml
[licenses]
allow = [
  "MIT", "Apache-2.0", "MIT-0", "BSD-2-Clause", "BSD-3-Clause",
  "ISC", "Zlib", "CC0-1.0", "Unlicense",
  "Unicode-3.0", "Unicode-DFS-2016",
  "CDLA-Permissive-2.0",   # webpki-roots (rustls trust anchors)
]
confidence-threshold = 0.9

[[ignored-crates]]         # filled in only with a written justification comment
```

Banned (must never appear): `*GPL*` with a version other than 3.0-only, `*AGPL*`, `*SSPL*`,
`OpenSSL`/`native-tls` (use rustls), `ring`-incompatible C crypto for musl.

### 2.2 Crates

| Crate | Version | Features | License | Used by |
|---|---|---|---|---|
| `axum` | 0.8 | `http2`, `multipart`, `tokio`, `query`, `form` | MIT | bk-serve |
| `askama` + `askama_axum` | 0.16 | `serde_json`, `urlencode` | MIT/Apache-2.0 | bk-serve |
| `tokio` | 1.54 | `rt-multi-thread`, `macros`, `signal`, `net`, `fs`, `io-util`, `sync`, `time`, `process` | MIT | all |
| `tower-http` | 0.7 | `trace`, `compression-gzip`, `compression-br`, `set-header` | MIT | bk-serve |
| `rusqlite` | 0.43 | `bundled` | MIT | bk-db |
| `reqwest` | 0.13 | **`default-features = false`** + `rustls-tls`, `http2`, `brotli`, `gzip`, `zstd`, `stream` | MIT/Apache-2.0 | bk-crawl |
| `texting_robots` | 0.2 | — | MIT/Apache-2.0 | bk-crawl |
| `scraper` | 0.29 | — | MIT | bk-crawl |
| `lol_html` | 3.0 | `html` | BSD-3-Clause | bk-crawl (optional stream-strip) |
| `encoding_rs` | 0.8 | `serde` | MIT/Apache-2.0 | bk-crawl |
| `whatlang` | 0.18 | — | MIT | bk-crawl |
| `url` | 2.5 | `serde` | MIT/Apache-2.0 | all |
| `idna` | 1.1 | — | MIT/Apache-2.0 | bk-crawl |
| `image` | 0.25 | `default-features=false`, `png`, `jpeg`, `webp` | MIT/Apache-2.0 | bk-serve |
| `argon2` | 0.6 | `std` | MIT/Apache-2.0 | bk-serve |
| `sha2` | 0.10 | `oid` | MIT/Apache-2.0 | bk-core/bk-serve |
| `base64` | 0.22 | `std` | MIT/Apache-2.0 | bk-serve/bk-cli |
| `rand` | 0.9 | `thread_rng` | MIT/Apache-2.0 | bk-serve/bk-core |
| `getrandom` | 0.3 | — | MIT/Apache-2.0 | bk-core |
| `memmap2` | 0.9 | — | MIT/Apache-2.0 | bk-model |
| `arc-swap` | 1.8 | — | MIT/Apache-2.0 | bk-infer |
| `xxhash-rust` | 0.8 | `xxh3` | MIT | bk-core/bk-crawl |
| `dashmap` | 6 | — | MIT | bk-crawl/bk-serve |
| `smallvec` | 1 | `union` | MIT/Apache-2.0 | bk-infer/bk-search |
| `rustc-hash` | 2 | — | MIT/Apache-2.0 | all |
| `aho-corasick` | 1 | — | MIT/Apache-2.0 | bk-search |
| `memchr` | 2 | — | MIT/Apache-2.0 | bk-search |
| `rayon` | 1 | — | MIT/Apache-2.0 | bk-db (index backfill only) |
| `mimalloc` | 0.1 | — | MIT | bk-cli |
| `rust-embed` | 8.14 | `compression`, `include-exclude` | MIT/Apache-2.0 | bk-serve |
| `serde` | 1 | `derive` | MIT/Apache-2.0 | all |
| `serde_json` | 1 | `preserve_order` | MIT/Apache-2.0 | all |
| `toml` | 0.9 | `parse`, `display` | MIT/Apache-2.0 | bk-core |
| `thiserror` | 2 | — | MIT/Apache-2.0 | bk-core |
| `tracing` | 0.1 | `attributes` | MIT | all |
| `tracing-subscriber` | 0.3 | `env-filter`, `fmt` | MIT | bk-cli/bk-serve |
| `clap` | 4 | `derive`, `env` | MIT/Apache-2.0 | bk-cli |
| `indicatif` | 0.18 | `improved_unicode` | MIT | bk-train/bk-cli |
| `tar` | 0.4 | — | MIT/Apache-2.0 | bk-ops |
| `zstd` | 0.13 | — | BSD-3-Clause | bk-ops |
| `divan` | 0.1 | `gen` | MIT/Apache-2.0 | benches |
| `proptest` | 1.12 | — | MIT/Apache-2.0 | tests |
| `burn` | **0.22** | `default-features=false`, `std`, `train`, `wgpu`, `tui` | MIT/Apache-2.0 | **bk-train only** |
| `futures` | 0.3 | `std` | MIT/Apache-2.0 | bk-crawl/bk-db |
| `tokio-util` | 0.7 | `rt`, `io` | MIT | bk-crawl |
| `tokio-stream` | 0.1 | `sync` | MIT | bk-serve |
| `crc32c` | — | **not used** — CRC32C implemented in-house (~40 lines) in `bk-model` | — | — |

Notes:
- `burn = "0.23.0-pre.2"` exists on crates.io but is a pre-release. Pin **0.22** (stable) and
  validate the API in task P5-1 before writing model code.
- `reqwest` must never be built with `default-tls`; a stray `native-tls` in the graph fails `cargo deny`.
- `lol_html` is optional: if `scraper` alone passes the readability fixtures, drop `lol_html`
  (record the deviation in the commit message).

---

## 3. Crate contracts (types every other crate compiles against)

These are the signatures to create in P0 (stubs) and fill in per phase. Keeping them stable is
what lets phases run in parallel.

### `bk-core`

```rust
pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("database: {0}")]      Db(String),
    #[error("http: {0}")]          Http(String),
    #[error("model: {0}")]         Model(String),
    #[error("crawl: {0}")]         Crawl(String),
    #[error("io: {0}")]            Io(#[from] std::io::Error),
    #[error("not found")]          NotFound,
    #[error("conflict: {0}")]      Conflict(String),
    #[error("validation: {0}")]    Validation(String),
    #[error("unauthorized")]       Unauthorized,
    #[error("forbidden")]          Forbidden,
    #[error("rate limited")]       RateLimited,
    #[error("internal: {0}")]      Internal(String),
}
impl Error {
    pub fn status(&self) -> u16;      // 400/401/403/404/409/429/500
    pub fn kind(&self) -> &'static str; // stable string for metrics/logs
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct Id(pub i64);

pub fn now() -> i64;                       // unix seconds
pub fn now_ms() -> i64;
pub fn xxh3(bytes: &[u8]) -> u64;
pub fn xxh3_str(s: &str) -> u64;

pub struct Config { /* serde + Default + load(path) -> Result<Config> */ }
```

### `bk-model`

```rust
pub enum ModelKind { Spell, Embed, Rank }
pub enum DType { Int8, F16, F32 }

pub struct Header {
    pub magic: [u8; 6],       // b"BKMDL1"
    pub version: u16,         // 1
    pub kind: ModelKind,
    pub arch: u32,            // bit0 AVX2, bit1 VNNI, bit2 NEON, bit3 fp16
    pub tensor_count: u32,
    pub vocab_len: u32,
    pub flags: u32,
}

pub struct Section {
    pub name: String, pub rank: u8, pub dims: Vec<u32>, pub dtype: DType,
    pub scale: f32, pub zero: i8, pub offset: u64, pub len: u64, pub crc: u32,
}

pub struct ModelFile { /* owns Mmap */ }

impl ModelFile {
    pub fn open(path: &Path) -> Result<Self>;
    pub fn from_bytes(&[u8]) -> Result<Self>;      // fuzz target entry point
    pub fn kind(&self) -> ModelKind;
    pub fn section(&self, name: &str) -> Result<TensorView<'_>>;
}

pub struct TensorView<'a> {
    pub dims: &'a [u32], pub dtype: DType, pub bytes: &'a [u8],
    pub scale: f32, pub zero: i8,
}
```

Validation order in `from_bytes`: magic → version → total length ≥ header → per-section
`offset+len ≤ file.len()` → CRC32C per section → quant range (`zero` in `[-128,127]`).
Any failure is `Error::Model`, never a panic. This function is the `cargo-fuzz` target.

### `bk-infer`

```rust
pub struct Scratch { /* thread-local reusable buffers */ }
impl Scratch { pub fn new() -> Self; }

pub struct SpellModel { /* Arc<ModelFile> */ }
impl SpellModel {
    pub fn open(path: &Path) -> Result<Self>;
    pub fn score(&self, typo: &str, cand: &str, s: &mut Scratch) -> f32;   // noisy channel
    pub fn correct(&self, q: &str, cands: &[String], s: &mut Scratch) -> Option<(String, f32)>;
}

pub struct EmbedModel { /* Arc<ModelFile> */ }
impl EmbedModel {
    pub fn open(path: &Path) -> Result<Self>;
    pub fn encode(&self, text: &str, out: &mut Vec<u8>, s: &mut Scratch) -> Result<()>; // int8 + scale
    pub fn dim(&self) -> usize;
}

pub struct RankModel { /* Arc<ModelFile> */ }
impl RankModel {
    pub fn open(path: &Path) -> Result<Self>;
    pub fn score(&self, feats: &[f32], s: &mut Scratch) -> f32;
}

// Kernels (all pub for differential tests)
pub fn int8_gemm(a: &[i8], ao: usize, b: &[i8], bo: usize, k: usize, out: &mut [i32]);
pub fn layer_norm(x: &[f32], gamma: &[f32], beta: &[f32], eps: f32, out: &mut [f32]);
pub fn gelu(x: &[f32], out: &mut [f32]);
pub fn softmax(x: &[f32], out: &mut [f32]);
pub fn maxpool1d(x: &[f32], k: usize, stride: usize, out: &mut [f32]);
pub fn conv1d_int8(x: &[i8], w: &[i8], b: &[i32], k: usize, stride: usize, out: &mut [i32]);
```

Kernel dispatch: `#[cfg(target_arch="x86_64")]` + `is_x86_feature_detected!("avx2")`,
`"avxvnni"`; `#[cfg(target_arch="aarch64")]` + `is_aarch64_feature_detected!("neon")`/`("sdot")`?
— **`sdot` is not a stable `is_aarch64_feature_detected!` key**; detect via `std::arch::is_aarch64_feature_detected!("neon")`
plus a runtime `cfg` probe or `hwcap` read at startup. Fallback: scalar `i32` accumulate with
`maddubs`-equivalent. Scalar path is always compiled and is the test oracle.

### `bk-db`

```rust
pub struct Db { /* r2d2-free: pool = Arc<Mutex<Connection>> for readers + Writer */ }

impl Db {
    pub fn open(path: &Path) -> Result<Self>;        // applies PRAGMAs + migrations
    pub fn migrate(&self) -> Result<()>;             // forward-only, embedded SQL
    pub fn writer(&self) -> Writer;                  // clone-cheap handle
    pub fn with_conn<R>(&self, f: impl FnOnce(&Connection) -> Result<R>) -> Result<R>;
    pub fn with_blocking<R: Send + 'static>(&self, f: impl FnOnce(&Connection) -> Result<R> + Send + 'static)
        -> impl Future<Output = Result<R>>;
}

pub struct Writer { /* mpsc::Sender<WriteJob> */ }
impl Writer {
    pub async fn exec<F>(&self, f: F) -> Result<()>
    where F: FnOnce(&Transaction<'_>) -> Result<()> + Send + 'static;
    pub async fn exec_batch<F>(jobs: Vec<F>) -> Result<()>; // one transaction
}

pub mod users;     pub mod sessions;   pub mod bookmarks;
pub mod tags;      pub mod screenshots; pub mod crawls;
pub mod vectors;   pub mod clicks;     pub mod shares;
pub mod fts;       pub mod meta;
```

Writer batching: accumulate `WriteJob`s, flush on **25 ms** or **512 jobs**, whichever first;
a job that returns `Err` aborts only its own transaction and the error is returned to that
caller (jobs are independent transactions inside the batch). Readers never block on the writer.

### `bk-crawl`

```rust
pub struct Article {
    pub url: String, pub final_url: String, pub site: String,
    pub title: String, pub byline: Option<String>, pub date: Option<i64>,
    pub text: String, pub lang: Option<String>,
    pub text_hash: u64, pub mime: Option<String>,
    pub boilerplate_ratio: f32,
}

pub enum CrawlError {
    Dns(String), Tls(String), Status(u16), TooManyRequests, Timeout,
    TooLarge(u64), Empty, UnknownCharset, NotHtml(String),
    Robots, Ssrf(String), RedirectLoop, Io(String),
}
impl CrawlError { pub fn kind(&self) -> &'static str; pub fn retryable(&self) -> bool; }

pub struct Fetcher { /* reqwest::Client + semaphore + per-host limiter + robots cache */ }
impl Fetcher {
    pub fn new(cfg: &Config) -> Result<Self>;
    pub async fn fetch(&self, url: &Url) -> Result<FetchOutcome, CrawlError>;
}

pub enum FetchOutcome { Article(Article), Binary { mime: String, bytes: u64 } }

pub fn url_norm(raw: &str) -> Result<String>;
pub fn ssrf_check(url: &Url, pinned: &mut Option<IpAddr>) -> Result<()>;
pub fn robots_allows(robots: &Robots, ua: &str, url: &Url) -> bool;
pub fn extract(html: &str, base: &Url) -> Article;   // no network, pure; fuzz target
pub fn detect_charset(bytes: &[u8], ct: Option<&str>) -> &'static encoding_rs::Encoding;
```

`CrawlError` ↔ `bk_core::Error::Crawl(kind)` conversion; the HTTP layer maps it to a user-visible
message + retry flag, never a raw error string.

### `bk-search`

```rust
pub struct SearchRequest { pub q: String, pub user_id: Id, pub limit: usize, pub offset: usize }
pub struct SearchHit {
    pub bookmark: Bookmark, pub score: f32, pub bm25: f32,
    pub vector_sim: Option<f32>, pub snippet: String, pub highlights: Vec<(usize, usize)>,
}
pub struct Breakdown { pub parse_us: u32, pub spell_us: u32, pub fts_us: u32,
                       pub vector_us: u32, pub rerank_us: u32, pub total_us: u32 }
pub struct SearchResponse {
    pub hits: Vec<SearchHit>, pub corrected_query: Option<String>,
    pub took_ms: u32, pub degraded: bool, pub breakdown: Breakdown,
}

pub struct ParsedQuery {
    pub terms: Vec<String>, pub phrases: Vec<String>, pub negated: Vec<String>,
    pub tags: Vec<String>, pub sites: Vec<String>, pub screenshots_only: bool,
    pub before: Option<i64>, pub after: Option<i64>,
}
pub fn parse_query(raw: &str) -> Result<ParsedQuery>;   // fuzz target
pub fn normalize(s: &str) -> String;                    // NFKC, casefold, strip diacritics, CJK split

pub struct SymSpell { /* per-user deletes-1/deletes-2 index */ }
impl SymSpell {
    pub fn build(vocab: impl Iterator<Item = (String, u32)>) -> Self;
    pub fn lookup(&self, term: &str, max_edit: u8) -> Vec<(String, u32)>;
}

pub struct Searcher { /* db + SymSpell + models + config */ }
impl Searcher {
    pub fn new(db: Db, models: ModelSet, cfg: Arc<Config>) -> Result<Self>;
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;
    pub fn refresh_vocab(&self, user_id: Id) -> Result<()>;  // after bulk import
}

pub struct ModelSet {
    pub spell: Option<Arc<SpellModel>>,
    pub embed: Option<Arc<EmbedModel>>,
    pub rank:  Option<Arc<RankModel>>,
}
impl ModelSet {
    pub fn load(models_dir: &Path) -> Self;   // never fails; missing/corrupt => None
    pub fn degraded(&self) -> bool;
}
```

Fusion (P4 lexical-only, P5 adds semantic):

```
score = 0.6 * rrf(lexical) + 0.4 * rrf(semantic)     // RRF k = 60
if semantic arm empty -> score = rrf(lexical)        // adaptive, no penalty
```

### `bk-serve`

```rust
pub struct AppState {
    pub db: Db, pub cfg: Arc<Config>, pub search: Arc<Searcher>,
    pub models: Arc<ModelSet>, pub fetcher: Arc<Fetcher>, pub limiter: Arc<RateLimiter>,
    pub csrf: Arc<CsrfTokens>,
}
pub struct AuthUser { pub id: Id, pub username: String, pub role: Role }
pub enum Role { Admin, User }

pub fn router(state: AppState) -> Router;   // every route mounted here, nothing outside
pub async fn mount_static() -> Router;      // rust-embed assets, pre-compressed
```

Route table is exactly §4 of `PLAN.md`. Mutations require `X-CSRF-Token` matching the
`bk_csrf` cookie (double-submit). `GET` never mutates. `/api/v1/*` mirrors the HTML routes.

### `bk-cli`

```rust
// crates/bk-cli/src/main.rs
#[derive(Parser)] enum Cmd {
    Init, Serve { host: String, port: u16 },
    Watch { max_restarts: Option<u32> },
    User { add: bool, list: bool, rm: bool, username: String },
    Add { target: String }, Search { query: String },
    Crawl { all: bool, limit: Option<usize> },
    Index { rebuild: bool },
    Export { out: PathBuf }, Import { input: PathBuf },
    Backup { out: Option<PathBuf> },
    Model { list: bool, verify: bool, train: TrainArgs },
    Doctor, Service { install: bool, start: bool, stop: bool, status: bool, logs: bool },
    Update,
}
```

`TrainArgs` is only compiled under `--features train`; without the feature, `bookmark model train`
prints a build hint and exits 2.

### `bk-train` (lib + bins, `--features train` only)

```rust
pub mod data;    // seed wordlist + corpus loader + synthetic typo generator + templates
pub mod spell;   // char-CNN/MLP noisy-channel training (Burn)
pub mod embed;   // 4-layer d=256 bi-encoder, InfoNCE + hard negatives
pub mod rank;    // MLP RankNet on click logs + synthetic labels
pub mod export;  // -> .bkmdl int8 (per-tensor symmetric, 512-sample calibration)
pub mod device;  // single WgpuDevice, DX12 preferred on Windows, cpu fallback
```

Bins: `bk-train-spell`, `bk-train-embed`, `bk-train-rank`, `bk-train-export`. The `bookmark train`
subcommand shells into these (or calls the same lib functions) so there is one code path.

---

## 4. Phase tasks

### P0 — Scaffold (0.5 day)

| ID | Files | Deliverable | Done when |
|---|---|---|---|
| P0-1 | `Cargo.toml`, `rust-toolchain.toml`, `.gitignore` | workspace + profiles + `default-members` | `cargo build` succeeds and does **not** compile `burn` |
| P0-2 | `deny.toml`, `LICENSE` | license gate + GPL-3.0 text | `cargo deny check licenses` exits 0 on the empty graph |
| P0-3 | `crates/bk-core/src/{lib,error,time,hash,config}.rs` | §3 contracts + stubs | `cargo test -p bk-core` green |
| P0-4 | `crates/bk-{model,infer,db,crawl,search,serve,train,cli,ops}/` | one `lib.rs`/`main.rs` stub each, correct `Cargo.toml` deps | `cargo build --workspace --exclude bk-train` green |
| P0-5 | `crates/bk-cli/src/main.rs` | `bookmark --version`, `--help` | `bookmark --version` prints `bookmark 0.1.0` |
| P0-6 | `README.md`, `docs/{ARCHITECTURE,MODEL_CARD,TERMUX,TAILSCALE,TROUBLESHOOTING}.md` | skeletons with TOC placeholders | files exist, no TODO-only sections left in README |
| P0-7 | `install.sh` | arch detect, sha256 verify, idempotent, `--dev`, `--uninstall`, `--yes` | `sh install.sh --dev` installs to `~/.local/bin` twice with no error |
| P0-8 | `.github/workflows/ci.yml` | fmt / clippy -D warnings / test / deny / audit | workflow parses; first run green |
| P0-9 | `.github/workflows/release.yml` | musl x86_64+aarch64, android (optional), windows `--features train`; sha256 | workflow parses |
| P0-10 | git | `git init`, repo-local `user.name=Vaibhav`, `user.email=vaibhav@users.noreply.github.com`, first commit, remote `github.com/thisisforlearn/bookmark`, push `main` | `git log -1` shows Vaibhav; `git ls-remote` returns `refs/heads/main` |

P0-10 note: the remote **already exists but has no refs** (verified with `git ls-remote`). Use
`git remote add origin …` + `git push -u origin main`; do **not** run `gh repo create` (it would
fail on an existing repo). Publishing is pre-authorized.

**P0 demo:** `cargo build && ./target/debug/bookmark --version && sh install.sh --dev && bookmark doctor`

---

### P1 — Core: schema, auth, CRUD, UI (1 day)

| ID | Files | Deliverable | Done when |
|---|---|---|---|
| P1-1 | `migrations/0001_init.sql`, `crates/bk-db/src/migrate.rs` | full DDL from §5, `user_version` gate | `bookmark init` creates the DB; `migrate` is idempotent on second run |
| P1-2 | `crates/bk-db/src/{pool,writer}.rs` | PRAGMAs, reader pool, batched writer | 200 concurrent writes produce **0** `SQLITE_BUSY`; one transaction per ≤25 ms/512 jobs |
| P1-3 | `crates/bk-db/src/{users,sessions}.rs` | argon2id (m=19456,t=2,p=1) PHC strings, 32-byte tokens, sha256 at rest, rotation | login → new session id, old token rejected |
| P1-4 | `crates/bk-db/src/{bookmarks,tags}.rs` | CRUD + `url_norm` unique per user | duplicate URL for same user → `Error::Conflict` |
| P1-5 | `crates/bk-serve/src/auth.rs` | register/login/logout, cookie `bk_session` (HttpOnly, SameSite=Lax, Secure under TLS), `AuthUser` extractor, invite-only flag | protected route without cookie → 401 + redirect to `/login` |
| P1-6 | `crates/bk-serve/src/security.rs` | CSP nonce, `X-Content-Type-Options`, `X-Frame-Options: DENY`, CSRF double-submit, per-user rate limit | all mutation routes reject a missing/bad CSRF token with 403 |
| P1-7 | `crates/bk-serve/src/routes/{bookmarks,search,health}.rs` | list/detail/create/delete, `/healthz`, `/search` (LIKE only) | CRUD works via HTML form; `/healthz` returns `{"ok":true}` |
| P1-8 | `crates/bk-serve/templates/**`, `assets/**` + `rust-embed` | base, login, register, list, detail, 404/500 | page renders with zero inline JS except the nonce'd htmx tag |
| P1-9 | `crates/bk-cli/src/cmd/{init,user,add,search,serve}.rs` | `init`, `user add/list/rm`, `add <url\|text>`, `search`, `serve` | `bookmark add "hello" && bookmark search hello` returns the row |
| P1-10 | `tests/route_smoke.rs` | axum `oneshot` per route, multi-user isolation | `cargo test --all` green |

**P1 demo:** `bookmark init && bookmark user add --admin me && bookmark serve` → log in, add a
text bookmark, see it in the list, `bookmark search` finds it.

---

### P2 — Crawl + index (1 day)

| ID | Files | Deliverable | Done when |
|---|---|---|---|
| P2-1 | `crates/bk-crawl/src/url.rs`, `ssrf.rs` | `url_norm` (lowercase host, strip `utm_*`/fragment/default ports, IDNA), SSRF guard (private/loopback/link-local/CGNAT/multicast, IP pin, re-check per redirect, ≤3) | unit tests for `127.0.0.1`, `169.254.169.254`, `10/8`, `100.64/10`, `[::1]`, redirect-to-private |
| P2-2 | `crates/bk-crawl/src/{robots,fetch}.rs` | robots cache + 1 req/host/1.5 s + global semaphore 4, 10 s connect / 15 s total, 2 MB cap, brotli/gzip/zstd | mock server test: 429 → backoff, 3 MB body → `TooLarge` |
| P2-3 | `crates/bk-crawl/src/charset.rs`, `readability.rs` | encoding_rs detect, script/style/nav strip, density + link-density + heading scoring, ≤400 KB text | fixture corpus (≥20 pages) extracts title/text within tolerance; fuzz target clean |
| P2-4 | `crates/bk-crawl/src/lang.rs`, `dedupe.rs` | whatlang + xxh3 + boilerplate ratio <8% → reject | `extract` never panics on malformed HTML |
| P2-5 | `migrations/0002_fts.sql`, `crates/bk-db/src/fts.rs` | `bookmark_fts` external-content + AI/AD/AU triggers, `fts_tri` trigram, vectors/crawls/meta tables | insert/update/delete bookmark → FTS rows match exactly |
| P2-6 | `crates/bk-db/src/{crawls,vectors,meta}.rs` | retry state, `next_try_at`, vector upsert, meta counters | failed crawl → `attempts+1`, exponential `next_try_at` |
| P2-7 | `crates/bk-cli/src/cmd/{crawl,index}.rs` | `crawl [--all --limit N]` with `indicatif` bar, resumable; `index --rebuild` | 100-fixture crawl completes; re-run resumes without refetching |
| P2-8 | `tests/crawl_mock.rs`, `tests/fixtures/**` | mock 200/403/429/timeout/huge/binary/charset server | all failure classes produce typed `CrawlError` + user-visible message |

**P2 demo:** `bookmark add https://example.com && bookmark crawl --all && bookmark search "example"`

---

### P3 — Screenshots, multi-user, share, backup (0.5 day)

| ID | Files | Deliverable | Done when |
|---|---|---|---|
| P3-1 | `crates/bk-serve/src/routes/upload.rs` | multipart (text ≤1 MB, screenshot ≤5 MB), magic-byte sniff, `image` decode + resize (max 1600 px) + thumbnail (320 px), sha256 dedupe | spoofed `.png` containing HTML is rejected 400 |
| P3-2 | `crates/bk-db/src/screenshots.rs` | blob + mime + dims + sha256, one per bookmark | re-upload same image → same row, no duplicate blob |
| P3-3 | `crates/bk-serve/src/routes/share.rs` | `shares` CRUD, `GET /share/:token`, expiry, revoke | expired token → 404; revoked token → 404 |
| P3-4 | `crates/bk-serve/src/routes/{export,import}.rs` | JSON export (+ optional base64 screenshots), validate + idempotent import by `url_norm` | export → wipe → import → identical set |
| P3-5 | `crates/bk-ops/src/backup.rs` | `tar.zst` snapshot + sha256 manifest, `bookmark backup` | restore into a fresh DB yields identical row counts |
| P3-6 | `crates/bk-serve/src/routes/settings.rs` | per-user settings JSON, role gate on user admin | non-admin cannot create users (403) |

**P3 demo:** upload a screenshot, open `/b/:id`, create a share link, `curl /share/:token` in a
private window, `bookmark backup` then `bookmark import`.

---

### P4 — Search v1 (0.5 day)

| ID | Files | Deliverable | Done when |
|---|---|---|---|
| P4-1 | `crates/bk-search/src/parse.rs` | quoted phrases, `tag:`, `site:`, `is:screenshot`, `before:`/`after:`, `-term` | proptest round-trip: `parse` → normalize → re-render is stable |
| P4-2 | `crates/bk-search/src/suggest.rs` | SymSpell deletes-1/2 over per-user + global vocab, LRU-capped, incremental update | beats brute force on a 10 k-word vocab (correctness diff) |
| P4-3 | `crates/bk-search/src/lexical.rs` | FTS5 unicode61+porter, trigram for ≥3-char substrings, `LIKE` fast path for 1–2 chars | 1-char and 2-char queries return non-zero rows; 10 k-doc p95 ≤ 25 ms |
| P4-4 | `crates/bk-search/src/rrf.rs`, `rerank_stub.rs` | weighted RRF fusion, `degraded` flag, placeholder ranker (BM25 only) | `SearchResponse` always includes `corrected_query`, `took_ms`, `degraded` |
| P4-5 | `crates/bk-search/src/snippet.rs` | highlight ranges + `did you mean` | snippet ≤ 240 chars, highlights non-overlapping |
| P4-6 | `benches/search.rs` (divan) | p50/p95, spell, crawl throughput | numbers recorded in `docs/PERF.md` |
| P4-7 | `crates/bk-cli/src/cmd/search.rs` | CLI prints corrected query + timing + degraded badge | `bookmark search "teh"` suggests "the" |

**P4 demo:** `bookmark index --rebuild && bookmark search "exmaple"` → corrected + results + badge.

---

### P5 — Models (2 days)

| ID | Files | Deliverable | Done when |
|---|---|---|---|
| P5-1 | `crates/bk-train/src/device.rs`, spike doc | confirm Burn 0.22 API (`Autodiff<Wgpu>`, wgpu options, schedulers), single `WgpuDevice`, CPU fallback, **serialized submissions** | `--backend cpu` completes 50 steps on WSL2; 10× smoke on DX12 with no crash |
| P5-2 | `crates/bk-model/src/{header,section,crc,validate,mmap}.rs`, `docs/BKMDL.md` | `.bkmdl` v1 reader + CRC32C + validation | truncated / bad-CRC / bad-version / bad-dim files all rejected without panic |
| P5-3 | `crates/bk-infer/src/{gemm,norm,act,conv,pool}.rs`, `dispatch.rs` | AVX2 + AVX-VNNI (`dpwssd`) + NEON `sdot` + scalar, runtime dispatch | differential test: SIMD vs scalar within 1 LSB after quant |
| P5-4 | `crates/bk-infer/src/{spell,embed,rank}.rs` | char-CNN spell scorer, 4-layer bi-encoder, MLP ranker | forward pass on a golden model matches Python-free reference fixtures |
| P5-5 | `crates/bk-train/src/data/{seed,typos,corpus}.rs` | embedded wordlist (1–2 MB), synthetic corruptions (keyboard/phone adjacency, OCR, phonetic, drop/transpose/dup, camelCase, URL noise), DB corpus merge | `bookmark train spell --dry-run` prints exact step/time estimate |
| P5-6 | `crates/bk-train/src/spell.rs` + bin | pairwise ranking + LM prior; checkpoint/resume | top-1 ≥ SymSpell baseline + 8 % on held-out synthetic typos |
| P5-7 | `crates/bk-train/src/embed.rs` + bin | InfoNCE, in-batch + BM25 hard negatives, 2 positives/query | nDCG@10 ≥ BM25 on held-out split |
| P5-8 | `crates/bk-train/src/rank.rs` + bin | RankNet on `clicks` + synthetic labels | nDCG@10 ≥ BM25-only |
| P5-9 | `crates/bk-train/src/export.rs` + bin | int8 per-tensor symmetric (512-sample calibration), fp16 embed output projection, vocab + thresholds + `trained_on` | reload with `bk-model`; max abs error ≤ 1 LSB vs fp32 |
| P5-10 | `models/*.bkmdl`, `models/*.json`, `models/eval/*.md` | default bundle < 8 MB, committed | `bookmark model verify` passes |
| P5-11 | `crates/bk-search/src/{semantic,rerank}.rs`, `rrf.rs` | vector arm (int8 dot over `vectors`), RRF lexical/semantic, LTR rerank top-100 | search works with models present **and** absent; `degraded=true` badge when absent |
| P5-12 | `crates/bk-infer/src/attention.rs` | MHA: QKᵀ → softmax → AV (needed by the 4-layer encoder; **not** in PLAN.md §7.6) | golden-model parity with P5-4 fixtures |
| P5-13 | `crates/bk-cli/src/cmd/model.rs` + `train` feature | `model list/verify/train/export`, progress bar + ETA + resume | `bookmark train spell --smoke` finishes < 5 min |

**P5 demo:** `bookmark train spell --backend cpu --smoke && bookmark model verify` →
`bookmark search "teh"` returns corrected results; then repeat with `models/` moved away and
confirm the degraded badge.

---

### P6 — Ops + release (0.5 day)

| ID | Files | Deliverable | Done when |
|---|---|---|---|
| P6-1 | `crates/bk-ops/src/supervisor.rs`, `cmd/watch.rs` | restart with 1 s→30 s backoff, `/healthz` every 10 s, notification after ≥3 crashes, `--max-restarts` | killing the server child restarts it; 3 kills emit one notification |
| P6-2 | `crates/bk-ops/src/paths.rs`, `log.rs` | 32 MB ring log with rotation, 0700 state dir | log never exceeds 32 MB |
| P6-3 | `crates/bk-ops/src/doctor.rs` | checks: db, migrations, model files, FTS5 availability, ports, tailscale, disk, perms, SIMD features | `bookmark doctor` exits non-zero on a broken install and explains the fix |
| P6-4 | `crates/bk-ops/src/{systemd,termux,windows}.rs` | `bookmark.service`, `~/.termux/boot/start-bookmark.sh`, Windows Scheduled Task | `bookmark service install` is idempotent |
| P6-5 | `install.sh` (release mode) | downloads static musl binary + sha256, installs, offers boot script + wake-lock | fresh Termux dry-run path is correct (no device here — reviewed against a shell test) |
| P6-6 | `README.md` rewrite | what it is → 3-step quickstart → Termux → start/stop/restart → LAN → Tailscale → systemd → models/training → backup → screenshots → troubleshooting → security → license | a first-time reader can go local → LAN → Tailscale without guessing |
| P6-7 | `.github/workflows/release.yml` | build musl x86_64/aarch64 + windows train, sha256, attach `install.sh` + `models/*.bkmdl` | first tagged release has all assets |
| P6-8 | git tag `v1.0.0` | tagged release on `main` | release page shows binaries + sums |

**P6 demo:** `bookmark doctor && bookmark service install && bookmark watch` under a killed child.

---

## 5. Target schema (DDL to write in P1-1/P2-5)

```sql
PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA busy_timeout=5000;
PRAGMA foreign_keys=ON;  PRAGMA mmap_size=268435456; PRAGMA cache_size=-32000;

CREATE TABLE users(
  id INTEGER PRIMARY KEY, username TEXT NOT NULL UNIQUE,
  pw_hash TEXT NOT NULL, role TEXT NOT NULL DEFAULT 'user',
  created_at INTEGER NOT NULL, settings_json TEXT NOT NULL DEFAULT '{}');
CREATE TABLE sessions(
  id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  token_hash TEXT NOT NULL UNIQUE, created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL, ip TEXT, ua TEXT);
CREATE TABLE bookmarks(
  id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  url_norm TEXT NOT NULL, url TEXT NOT NULL,
  title TEXT NOT NULL DEFAULT '', description TEXT NOT NULL DEFAULT '',
  text TEXT NOT NULL DEFAULT '', text_hash INTEGER NOT NULL DEFAULT 0,
  lang TEXT, site TEXT NOT NULL DEFAULT '', status TEXT NOT NULL DEFAULT 'new',
  note TEXT NOT NULL DEFAULT '', pinned INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, crawled_at INTEGER,
  UNIQUE(user_id, url_norm));
CREATE TABLE tags(
  id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  name TEXT NOT NULL, color TEXT NOT NULL DEFAULT '', UNIQUE(user_id, name));
CREATE TABLE bookmark_tags(
  bookmark_id INTEGER NOT NULL REFERENCES bookmarks(id) ON DELETE CASCADE,
  tag_id INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE, PRIMARY KEY(bookmark_id, tag_id));
CREATE TABLE screenshots(
  bookmark_id INTEGER PRIMARY KEY REFERENCES bookmarks(id) ON DELETE CASCADE,
  mime TEXT NOT NULL, width INTEGER NOT NULL, height INTEGER NOT NULL,
  bytes INTEGER NOT NULL, sha256 TEXT NOT NULL, blob BLOB NOT NULL);
CREATE TABLE crawls(
  id INTEGER PRIMARY KEY, bookmark_id INTEGER NOT NULL REFERENCES bookmarks(id) ON DELETE CASCADE,
  http_status INTEGER, error_kind TEXT, attempts INTEGER NOT NULL DEFAULT 0,
  next_try_at INTEGER NOT NULL DEFAULT 0, bytes INTEGER NOT NULL DEFAULT 0, ms INTEGER NOT NULL DEFAULT 0);
CREATE TABLE vectors(
  bookmark_id INTEGER NOT NULL REFERENCES bookmarks(id) ON DELETE CASCADE,
  model_id TEXT NOT NULL, dim INTEGER NOT NULL, quant TEXT NOT NULL DEFAULT 'int8',
  scale REAL NOT NULL DEFAULT 1, blob BLOB NOT NULL, PRIMARY KEY(bookmark_id, model_id));
CREATE TABLE clicks(
  id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  query TEXT NOT NULL, bookmark_id INTEGER NOT NULL REFERENCES bookmarks(id) ON DELETE CASCADE,
  rank INTEGER NOT NULL, ts INTEGER NOT NULL);
CREATE TABLE shares(
  id INTEGER PRIMARY KEY, user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  bookmark_id INTEGER NOT NULL REFERENCES bookmarks(id) ON DELETE CASCADE,
  token TEXT NOT NULL UNIQUE, expires_at INTEGER);
CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE INDEX bookmarks_user_created ON bookmarks(user_id, created_at DESC);
CREATE INDEX bookmarks_user_site   ON bookmarks(user_id, site);
CREATE INDEX bookmarks_user_url    ON bookmarks(user_id, url_norm);
CREATE INDEX vectors_model         ON vectors(model_id);
CREATE INDEX clicks_user_ts        ON clicks(user_id, ts);
CREATE INDEX crawls_due            ON crawls(next_try_at);

CREATE VIRTUAL TABLE bookmark_fts USING fts5(
  title, description, text, content='bookmarks', content_rowid='id',
  tokenize="unicode61 remove_diacritics 2");
CREATE TRIGGER bookmarks_ai AFTER INSERT ON bookmarks BEGIN
  INSERT INTO bookmark_fts(rowid,title,description,text) VALUES(new.id,new.title,new.description,new.text); END;
CREATE TRIGGER bookmarks_ad AFTER DELETE ON bookmarks BEGIN
  INSERT INTO bookmark_fts(bookmark_fts,rowid,title,description,text) VALUES('delete',old.id,old.title,old.description,old.text); END;
CREATE TRIGGER bookmarks_au AFTER UPDATE ON bookmarks BEGIN
  INSERT INTO bookmark_fts(bookmark_fts,rowid,title,description,text) VALUES('delete',old.id,old.title,old.description,old.text);
  INSERT INTO bookmark_fts(rowid,title,description,text) VALUES(new.id,new.title,new.description,new.text); END;

CREATE VIRTUAL TABLE fts_tri USING fts5(title, body, tokenize='trigram');
```

P1 verification task: `SELECT sqlite_compileoption_used('ENABLE_FTS5')` must return `1` with
`rusqlite` `bundled`. If it returns `0`, add the build workaround (env `SQLITE_ENABLE_FTS5=1`
or a `libsqlite3-sys` feature) **in P1**, not later.

---

## 6. Verification commands (run at every phase boundary)

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --all
cargo deny check licenses
cargo audit
cargo build -p bk-cli --no-default-features          # musl-equivalent feature set
cargo build -p bk-cli --features train               # Windows training binary
```

---

## 7. Risks / pre-mortem

| Risk | Trigger | Mitigation built into the plan |
|---|---|---|
| `burn 0.22` API differs from PLAN.md's notes | P5-1 spike fails | P5-1 exists before any model code; `--backend cpu` keeps progress moving; CPU fallback is first-class |
| Intel Iris Xe heap corruption on ≥3 concurrent dispatches | P5-1 10× smoke crashes | one `WgpuDevice` per process, serialized submissions, `--backend cpu` on WSL2; real timing only on Windows 11 |
| `cargo deny` rejects a transitive crate | P0-2 fails | matrix in §2.2 is pre-filtered; swap is a one-line version/feature change, `lol_html` is the only non-required dep |
| GPL-3.0-only conflicts with a future dep | any new crate | adding a crate requires a matrix row with an explicit license check in the same commit |
| `rusqlite bundled` lacks FTS5 | P1 migration test | explicit P1 verification task + documented build workaround |
| `reqwest` pulls `native-tls` | musl build fails | `default-features = false`; `cargo tree -i native-tls` must be empty |
| `bk-train` (Burn) leaks into the server build | Termux binary balloons / fails | `default-members` + `train` feature gate; CI asserts `cargo tree -p bk-cli` has no `burn` |
| Writer batching adds latency to interactive adds | htmx feels slow | 25 ms cap + immediate flush when the channel is empty; readers never block |
| Trigram tokenizer misses 1–2 char queries | search returns 0 rows | P4-3 `LIKE` fast path with a measured fallback, documented in `docs/PERF.md` |
| `sdot` detection is unavailable in stable `std` | aarch64 path falls back | scalar fallback is always compiled and is the test oracle; `sdot` use is behind a runtime probe |
| No Android device in this environment | Termux path unverifiable here | P6-5 is a shell-level dry run + review; device smoke test is a documented user step |
| Scope creep past v1 | mid-project feature requests | phase boundaries are the contract; anything outside §4 waits for v1 |

---

## 8. Open questions (defaults chosen; change on request)

1. **Git author email** — `vaibhav@users.noreply.github.com`. No real address was supplied.
2. **Repo name** — `thisisforlearn/bookmark` (exists, empty). The directory is `bookmark-server`;
   the remote name stays `bookmark` unless told otherwise.
3. **Invite-only registration** — default; `POST /register` is compiled but disabled by config.
4. **Models in git** — default yes (~8 MB, no LFS). LFS would break `curl | bash` on Termux.
5. **Askama 0.16 + `askama_axum`** — assumed the axum integration crate version matches; verify at P1-8.
6. **`tower-http 0.7` vs `axum 0.8` tower version** — both should be tower 0.5/0.6 compatible; if
   not, drop `tower-http` and hand-roll the two middlewares (trace + compression).
7. **`lol_html` optional** — dropped if `scraper` alone passes the readability fixtures.