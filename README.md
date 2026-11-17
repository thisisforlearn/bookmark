# bookmark

A self-hostable, multi-user bookmark server that keeps **text, links, screenshots** and a
**searchable copy of every page**, with typo-tolerant search powered by small models you can
train yourself. One static binary. Runs on a laptop, a server, or a phone via Termux.

[Quickstart](#quickstart) · [Install as an app](#install-as-an-app) · [Termux](#termux-android) · [Windows](#windows) · [LAN & Tailscale](#lan-and-tailscale) ·
[Search syntax](#search-syntax) · [Models](#models-and-training) · [Backup](#backup-and-export) ·
[Security](#security) · [Troubleshooting](#troubleshooting) · [License](#license)

```
◆ bookmark        [ search bookmarks…              ]        Settings   Sign out
───────────────────────────────────────────────────────────────────────────────
 42 results · 4 ms
                                                            
 ⬤ Rust async runtimes            rust-lang.org/…
   Notes on the tokio executor and the async ecosystem…
   rust  async

 ⬤ Example Domain                 example.com
   This domain is for use in documentation examples…
```

---

## Why

Most bookmark tools are either a browser feature (locked to one browser, no full-text search)
or a hosted SaaS (your reading history on someone else's disk). This is neither:

- **Full-text search over page content**, not just titles. The server fetches each URL, strips
  the chrome, and indexes the article text with SQLite FTS5.
- **Typo-tolerant.** `ruts` finds `rust`. Two spell engines: SymSpell (always on) and an
  optional trained model that re-ranks candidates.
- **Yours.** SQLite file on your disk. No external services, no telemetry, no accounts on
  someone else's server.
- **Small.** ~14 MB static binary, < 45 MB idle RSS, single file database.
- **Multi-user** with per-user isolation, share links, and admin-managed accounts.

## Features

| | |
|---|---|
| Bookmarks | URLs, plain text notes, tags, pinning, notes |
| Crawling | Server-side fetch, readability extraction, robots.txt, SSRF guard, retries |
| Screenshots | Client uploads PNG/JPEG/WebP; auto-resized, deduplicated by SHA-256 |
| Installable app | PWA with an Android **share target** — share a link to save it |
| Quality signal | Thin/junk pages are scored and sink below real articles |
| Search | FTS5 + trigram, SymSpell typo correction, query operators, re-ranking |
| Models | Spell corrector, embedding encoder, LTR re-ranker as `.bkmdl`; download or train |
| Sharing | Revocable public share links with optional expiry |
| Data | JSON export/import, checksummed `.tar.zst` backups |
| Ops | `bookmark doctor`, watchdog, systemd unit, Termux boot hook |

---

## Quickstart

Three steps, about a minute:

```sh
# 1. install (Linux x86_64/aarch64, or Termux)
curl -fsSL https://github.com/thisisforlearn/bookmark/releases/latest/download/install.sh | bash

# 2. create your account (prints a generated password once)
bookmark init

# 3. run it
bookmark serve --host 127.0.0.1 --port 8080
```

Open <http://127.0.0.1:8080>, sign in, and paste a URL into **Add a bookmark**.

The installer also downloads the default search models, so typo correction and semantic
search work immediately. To fetch or refresh them later:

```sh
bookmark model install      # downloads the latest models from the release
```

### From source

```sh
git clone https://github.com/thisisforlearn/bookmark
cd bookmark
cargo build --release
./target/release/bookmark init
./target/release/bookmark serve
```

Requirements: Rust 1.97+. SQLite is bundled — nothing else to install.

### Windows

Download `bookmark-x86_64-pc-windows-gnu.exe` from the
[releases page](https://github.com/thisisforlearn/bookmark/releases), or build locally
with the GNU toolchain (no Visual Studio needed):

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup override set stable-x86_64-pc-windows-gnu
winget install Kitware.CMake NASM.NASM     # needed by the TLS + SQLite C code
cargo build --release -p bk-cli
```

If you see `linker link.exe not found`, you are on the MSVC target without Visual Studio
Build Tools. See [docs/WINDOWS.md](docs/WINDOWS.md) for all three options.

---

## Install as an app

bookmark is a progressive web app, so it installs to your phone's home screen and
registers itself in Android's **Share** menu — no APK, no app store.

1. Open the server URL in Chrome (or Safari on iOS).
2. Chrome shows an **Install** prompt; or use *⋮ → Add to Home screen*, or the
   **Install app** button on the Settings page.
3. Long-press any link in another app → **Share → bookmark**. The link is saved to
   your library instantly, with a toast confirming it.

That second step is the whole point: saving becomes one gesture from anywhere.

The same app works against a server on your LAN, over Tailscale, or at
`http://127.0.0.1:8080` on the device itself. Two common setups:

| Setup | Who it's for | How |
|---|---|---|
| **All-in-one** | Most people | Run bookmark on a laptop/Pi, install the PWA on your phone, point it at the LAN/Tailscale URL. Models come from the installer. |
| **Nerd mode** | You | Run bookmark *on the phone* in Termux (`bookmark serve`), models included, then install the PWA from `http://127.0.0.1:8080`. Nothing leaves the device. |

### Native APK

If you want a real app icon and the Android Share sheet, releases also include two APKs:

| APK | Size | Use |
|---|---|---|
| `bookmark-consumer.apk` | ~7 MB | **Zero setup** — the library runs on the phone |
| `bookmark-nerd.apk` | ~7 MB | Connects to a server you run, or serves locally |

Download from the [releases page](https://github.com/thisisforlearn/bookmark/releases), then
pair it with your server via **Settings → Connect a phone**. Full details in
[docs/ANDROID.md](docs/ANDROID.md).

---

## Termux (Android)

Termux gives you a real Linux userland on your phone. The static musl build runs there
unmodified.

```sh
pkg install curl
curl -fsSL https://github.com/thisisforlearn/bookmark/releases/latest/download/install.sh | bash
bookmark init
bookmark serve --host 127.0.0.1 --port 8080
```

Then open `http://127.0.0.1:8080` in your phone browser.

The installer automatically picks the Android (Bionic) build on Termux so the crawler can
resolve hostnames — Android has no `/etc/resolv.conf`, which a static musl binary needs.

**Keep it alive.** Android kills background processes aggressively:

```sh
bookmark service install     # writes ~/.termux/boot/start-bookmark.sh
pkg install termux-boot      # + install the Termux:Boot app, open it once
termux-wake-lock             # keep the CPU awake while charging
bookmark watch &             # supervisor: restarts on crash with backoff
```

See [docs/TERMUX.md](docs/TERMUX.md) for battery, storage, and reboot details.

---

## Starting, stopping, restarting

| Task | Command |
|---|---|
| Run in foreground | `bookmark serve` |
| Run with auto-restart | `bookmark watch` |
| Install as a service | `bookmark service install` |
| Check health | `bookmark doctor` |
| View logs | `journalctl --user -u bookmark -f` (systemd) or `$STATE/bookmark.log` |
| Stop | Ctrl-C, or `systemctl --user stop bookmark` |

`bookmark watch` restarts the server with exponential backoff (1s → 30s), sends a Termux
notification after repeated crashes, and honours `--max-restarts N`.

---

## LAN and Tailscale

By default the server binds **loopback only**. To reach it from other devices:

```sh
bookmark serve --host 0.0.0.0 --port 8080
```

Then `http://<your-lan-ip>:8080`. Only do this on a network you trust — the login is
password-protected but the traffic is plain HTTP.

### Tailscale (recommended for remote access)

[Tailscale](https://tailscale.com) gives you a private WireGuard network with HTTPS and no
port forwarding:

```sh
tailscale up                            # on the server
bookmark serve --host 0.0.0.0 --port 8080 &
tailscale serve --bg 8080               # HTTPS on https://<machine>.<tailnet>.ts.net
```

Now the UI is reachable from any of your devices at the tailnet URL, encrypted, and not exposed
to the public internet. Optionally lock the server to the tailnet:

```toml
# ~/.config/bookmark/config.toml
require_tailscale = true
```

See [docs/TAILSCALE.md](docs/TAILSCALE.md).

### systemd (Linux)

```sh
bookmark service install
systemctl --user daemon-reload
systemctl --user enable --now bookmark
loginctl enable-linger $USER     # start on boot
```

---

## Search syntax

| Query | Meaning |
|---|---|
| `rust async` | both words (AND), falls back to OR if nothing matches |
| `"exact phrase"` | phrase match |
| `tag:work` | bookmarks tagged `work` |
| `site:example.com` | only that domain |
| `is:screenshot` | only bookmarks with a screenshot |
| `before:2024-01-01` / `after:2024-01-01` | date range |
| `-draft` | exclude a word |

Results show **did you mean** when a typo is corrected, a **degraded** badge when models are
missing, and timing. Every result is fused from a lexical arm (FTS5) and, when an embedding
model is installed, a semantic arm.

---

## Models and training

The server works with **zero models** — it falls back to FTS5 + SymSpell. Models add:

1. **Spell model** — re-ranks typo candidates beyond raw edit distance.
2. **Embedding model** — semantic recall for paraphrases, so `concurrency` finds a note about
   async runtimes.
3. **LTR re-ranker** — replaces the hand-tuned scoring with a learned one.

All three live in one `.bkmdl` container per model under your models directory. They are
loaded with `mmap`, validated with CRC32C, and never block the query path.

### Train your own

Training runs entirely in Rust — no Python, no PyTorch, no GPU required.

```sh
# download prebuilt models (no training, recommended for most people)
bookmark model install

# quick sanity check (~seconds)
bookmark model train all --mode smoke

# usable models (~1–2 minutes on a laptop)
bookmark model train all --mode quick

# best quality (several minutes)
bookmark model train all --mode full

# one model at a time
bookmark model train spell --mode full
bookmark model verify
```

Training uses your bookmarks' text plus a built-in seed vocabulary, so it works on a fresh
install and improves as you save more. On Android it works but is **slow** — download or
train the models on a computer, then copy the `.bkmdl` files to the phone. The spell model is a char-n-gram MLP trained with SGD on
synthetic keyboard-typo pairs; the embedding model is a deterministic hashing vectorizer over
character n-grams.

Restart the server after training — the models load at startup and the UI stops showing the
**degraded** badge.

### Where models live

```
$DATA_DIR/models/
├── spell.bkmdl     ~30 KB
├── embed.bkmdl     ~90 KB (grows with your vocabulary)
└── rank.bkmdl      ~0.5 KB
```

`bookmark model list` shows what's installed; `bookmark model verify` checks every file's
magic, version, and per-tensor CRC.

See [docs/MODEL_CARD.md](docs/MODEL_CARD.md) for architectures and evaluation.

---

## Backup and export

```sh
bookmark backup                 # -> $DATA_DIR/bookmark-backup-<ts>.tar.zst
bookmark export out.json        # portable JSON (bookmarks, tags, notes)
bookmark import out.json        # idempotent by normalized URL
```

A backup is a compressed tarball containing `bookmark.db` and a `manifest.json` with its
SHA-256. To restore, move the old database aside and run `bookmark restore`-equivalent by
extracting the archive into the data directory (the CLI refuses to overwrite an existing DB).

Schedule it with cron or `termux-job-scheduler`:

```sh
0 3 * * *  bookmark backup >/dev/null 2>&1
```

---

## Screenshots

There is no headless browser in the binary. Screenshots are **uploaded by the client** — the
detail page has an upload control, and any tool that can POST multipart (`curl`, a bookmarklet,
a browser extension) works:

```sh
curl -b cookies.txt -F csrf=$CSRF -F image=@shot.png \
     http://127.0.0.1:8080/bookmarks/42/screenshot
```

Images are validated by decoding (not by extension), resized to ≤ 1600 px, and stored once,
deduplicated by SHA-256.

---

## Security

- Argon2id password hashing (19 MiB, t=2, p=1 — OWASP params).
- Sessions are 32 random bytes; only a SHA-256 is stored, so a DB leak exposes no live tokens.
- CSRF double-submit token on every mutating request.
- Strict CSP (`default-src 'self'`), `nosniff`, `frame-ancestors 'none'`.
- SSRF guard: private/loopback/link-local/CGNAT addresses blocked, re-checked on redirects.
- Crawler honours robots.txt, caps body size at 2 MB, and times out.
- Uploads are size-capped and decoded through the `image` crate.
- Invite-only by default (`open_registration = false`).
- State directory is created `0700`.

Report a vulnerability privately via GitHub Security Advisories.

---

## Architecture

```
bk-cli ──> bk-serve ──> bk-search ──> bk-infer ──> bk-model
  │           │             │             │
  │           ├──> bk-db ───┘             └── int8 kernels (AVX2 / AVX-VNNI / NEON)
  │           └──> bk-crawl (fetch, robots, readability)
  └──> bk-ops (backup, doctor, service files)
        └──> bk-train (spell / embed / rank trainers, exported to .bkmdl)
```

- **bk-db** — SQLite with WAL, a single batched writer task, FTS5 sync via triggers.
- **bk-crawl** — reqwest + rustls, streaming HTML strip, density-based readability.
- **bk-search** — parse → correct → lexical + semantic → RRF fuse → re-rank → snippet.
- **bk-infer** — pure Rust int8 GEMM and model forward passes; scalar oracle + SIMD paths.
- **bk-model** — the `.bkmdl` container: mmap, CRC32C, zero-copy tensor views.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

---

## Troubleshooting

| Symptom | Fix |
|---|---|
| `linker link.exe not found` (Windows) | You're on the MSVC target without VS — use the GNU toolchain, see [docs/WINDOWS.md](docs/WINDOWS.md) |
| `bookmark: command not found` | Add `~/.local/bin` (or `$PREFIX/bin`) to `PATH` |
| Server exits immediately | `bookmark doctor` — usually a taken port or unreadable data dir |
| Search shows **degraded** | Models not installed — `bookmark model train all --mode quick` |
| Crawl fails with "blocked" | The URL resolves to a private IP (SSRF guard) |
| Termux kills it | `termux-wake-lock`, `bookmark watch`, install Termux:Boot |
| Forgot password | `bookmark user add newadmin --admin` with `BOOKMARK_USER_PASSWORD=…` |
| Corrupt database | Restore from a `bookmark backup` archive |

More in [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md).

---

## License

GPL-3.0-only. © 2026 Vaibhav.

You can run it for anything, modify it, and share it — but distributed derivatives must also be
GPL-3.0-only. Dependency licenses are enforced in CI with `cargo deny`.
