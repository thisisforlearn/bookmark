# Running on Termux (Android)

## Install

```sh
pkg update
pkg install curl
curl -fsSL https://github.com/thisisforlearn/bookmark/releases/latest/download/install.sh | bash
```

The installer detects `aarch64`/`armv7`, downloads the matching **static musl** binary,
verifies its SHA-256, installs to `$PREFIX/bin/bookmark`, and offers to create the boot hook.

## First run

```sh
bookmark init          # creates ~/.bookmark, the DB, and prints an admin password
bookmark serve --host 127.0.0.1 --port 8080
```

Open `http://127.0.0.1:8080` in the phone browser.

## Keep it alive

Android will kill background processes to reclaim memory. Three things help:

1. **Wake lock** — stops the CPU from sleeping while the screen is off:
   ```sh
   termux-wake-lock
   ```
2. **Supervisor** — restarts on crash with exponential backoff:
   ```sh
   bookmark watch &
   ```
3. **Boot hook** — starts after a reboot:
   ```sh
   bookmark service install     # writes ~/.termux/boot/start-bookmark.sh
   ```
   Install the **Termux:Boot** app from F-Droid and open it once so the hook is registered.

Battery optimisation: exempt Termux from Android's battery optimisation
(Settings → Apps → Termux → Battery → Unrestricted), otherwise the OS may still kill it.

## Notifications

`bookmark watch` calls `termux-notification` when the server crashes repeatedly. No extra
configuration is needed if Termux:API is installed.

## Access from other devices

Phones change IPs. Two options:

- **Tailscale** (recommended): `pkg install tailscale` is not available, but the Android
  Tailscale app works — join the tailnet, then
  ```sh
  bookmark serve --host 0.0.0.0 --port 8080
  ```
  and reach it at the phone's tailnet IP.
- **LAN**: same command, then `http://<phone-ip>:8080` from a laptop on the same Wi-Fi.

## Storage

The database lives at `~/.bookmark/bookmark.db` (Termux home). For large libraries, check free
space with `bookmark doctor`. Backups are written beside the database.

## Why the installer picks an Android build

Android has no `/etc/resolv.conf`, and Android's DNS is resolved through the `netd`
daemon via Bionic libc. A **static musl** binary carries musl's own resolver, which
reads `/etc/resolv.conf` — so on Termux it can serve the UI and search perfectly but
**cannot resolve hostnames to crawl them**.

The installer therefore downloads `bookmark-aarch64-linux-android` on Termux, which is
linked against Bionic and resolves DNS exactly like `curl` and `pkg` do. The musl build
is still published for Alpine and other glibc-free Linux systems.

You can confirm which build you have:

```sh
bookmark doctor        # "SIMD kernels" and "version" lines
ldd "$(command -v bookmark)"   # musl build prints "not a dynamic executable"
```

## Models

```sh
bookmark model install          # recommended: download the prebuilt models
```

Training *works* on the phone but is much slower than on a laptop — the pure-Rust trainer
is single-threaded and phones throttle. If you want custom models, train on a computer and
copy `spell.bkmdl`, `embed.bkmdl`, `rank.bkmdl` into `~/.bookmark/models/`.

```sh
# on a computer, then copy the files over
bookmark model train all --mode full
```

## Notes

- The Android build is smaller and faster to start than the musl one because it uses
  Bionic's allocator and dynamic libc.
- `rustup` does not support `aarch64-linux-android` as a host, so building on-device requires
  `pkg install rust`. The prebuilt binary avoids that entirely.
- Cross-compiling locally needs an NDK or musl toolchain; CI builds these targets.
- The crawler respects `robots.txt` and a 2 MB body cap, which keeps mobile data usage sane.
