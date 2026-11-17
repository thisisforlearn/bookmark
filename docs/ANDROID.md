# Android app

Two APKs are built by `.github/workflows/android.yml` and attached to releases.

| APK | Size | What it does |
|---|---|---|
| `bookmark-consumer.apk` | ~7 MB | **Zero setup.** Runs the library on the phone. No address, no password. |
| `bookmark-nerd.apk` | ~7 MB | Connects to a server you run (Termux, a Pi, a laptop) and can also serve locally |

The PWA remains the zero-install option; the APK exists for the Share menu and for
running everything on-device.

## Install

Download from the [releases page](https://github.com/thisisforlearn/bookmark/releases):
`bookmark-consumer.apk` or `bookmark-nerd.apk`. Android will warn that the app is from an
unknown developer — that is expected for sideloaded APKs.

The APKs are signed with a debug keystore, which is fine for personal use but not accepted by
stores.

## Consumer app: nothing to set up

Install it and open it. On first launch it starts its own local library, creates a device
token for itself, and goes straight to the Library screen. Everything is stored in the app's
private files directory — no account, no address, no pairing code.

The first launch takes a second or two; you'll see "Setting up your library…".

## Nerd app: connect to your server

1. On the server, open **Settings → Connect a phone**.
2. The page shows the server address and an 8-character code (valid 10 minutes, single use).
3. In the app, enter the address and the code, tap **Connect**.

The app stores a device token; no password is typed and no cookie is shared.

From the server's shell you can also mint a token directly:

```sh
bookmark device add phone    # prints a token, nothing else
bookmark device list
```

## Saving things

| Action | How |
|---|---|
| Save a link | Any app → Share → **bookmark** |
| Save selected text | Select text → ⋮ → **Save to bookmark** |
| Save a screenshot | Screenshot → **Markup** (circle/draw) → Share → **bookmark** |
| Save the clipboard | App → Capture → *Save clipboard text* |
| Save manually | App → Add → paste a URL or text |
| Upload an image | App → Capture → *Choose a screenshot* |

All of these land in the library immediately and appear in web search.

## Backup and restore

Settings → **Back up** writes a zip of the whole library (database + models) wherever you
choose — Files, Drive, a USB stick. **Restore** reads one back; the library restarts
automatically. Both use Android's own file picker, so no storage permission is needed.

## Nerd mode

Both APKs ship the `aarch64-linux-android` server binary and the three models.

1. Open the app → **Settings → On-device server → Start**.
2. A notification appears; the server listens on `127.0.0.1:8080` only.
3. The app shows the generated admin password (first run only).
4. Open `http://127.0.0.1:8080` in the phone browser, sign in, then
   **Settings → Connect a phone** to pair the app with the local server.

Data lives in the app's private `files/` directory. Uninstalling removes it.

How it works: Android 10+ blocks `exec()` from app data (W^X), but allows it from
`nativeLibraryDir`. The server binary is packaged as `libbookmark.so` with
`useLegacyPackaging = true`, so the installer extracts it to a read-only directory that is
executable. The process runs inside a `dataSync` foreground service and is killed when the
service stops.

If a future Android release blocks this, nerd mode degrades to typing your Termux server
address — the consumer code path.

## Building locally (optional)

You need JDK 17 and the Android SDK. First build downloads roughly 1 GB.

```bash
cd android
gradle assembleConsumerRelease
# nerd flavor additionally needs the binary:
mkdir -p app/src/nerd/jniLibs/arm64-v8a
curl -L -o app/src/nerd/jniLibs/arm64-v8a/libbookmark.so \
  https://github.com/thisisforlearn/bookmark/releases/latest/download/bookmark-aarch64-linux-android
chmod +x app/src/nerd/jniLibs/arm64-v8a/libbookmark.so
gradle assembleNerdRelease
```

Outputs: `android/app/build/outputs/apk/<flavor>/release/`.

## What is deliberately not included

- **OkHttp / Retrofit / Room / Coil / ML Kit** — the API client is `HttpURLConnection` +
  `org.json`, storage is `SharedPreferences`, images use `BitmapFactory`. This keeps the APK
  around 3 MB instead of 10–15 MB.
- **CameraX** — image capture goes through the system screenshot editor and the picker.
- **A crop/annotation library** — Android's own Markup editor already draws circles.
