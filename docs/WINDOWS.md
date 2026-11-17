# Building and running on Windows

## Why you saw `linker link.exe not found`

That error is not a bug in bookmark — it means Cargo picked the **MSVC** target
(`x86_64-pc-windows-msvc`), which needs Microsoft's linker from Visual Studio Build
Tools. If you don't have Visual Studio installed, Cargo cannot link anything, so even
build scripts fail.

You have three options. Pick one.

---

## Option A — use a prebuilt binary (no compiler at all)

Every release ships a self-contained Windows executable:

<https://github.com/thisisforlearn/bookmark/releases>

Download `bookmark-x86_64-pc-windows-gnu.exe`, rename it to `bookmark.exe`, and run:

```powershell
.\bookmark.exe init
.\bookmark.exe serve --host 127.0.0.1 --port 8080
```

This is the fastest path if you just want to use it.

---

## Option B — build with the GNU toolchain (recommended if you want to compile)

The Rust **GNU** toolchain ships its own MinGW linker, so you get a working build
without Visual Studio.

```powershell
# 1. install the GNU toolchain (bundles x86_64-w64-mingw32-gcc as the linker)
rustup toolchain install stable-x86_64-pc-windows-gnu

# 2. pin it for this project only
cd D:\see\bookmark
rustup override set stable-x86_64-pc-windows-gnu

# 3. two native build helpers are needed by the TLS/SQLite C code
winget install Kitware.CMake
winget install NASM.NASM
#    (then close and reopen the terminal so PATH updates)

# 4. build
cargo build --release -p bk-cli

# 5. run
.\target\release\bookmark.exe init
.\target\release\bookmark.exe serve
```

Verify you are on the right toolchain at any point:

```powershell
rustup show active-toolchain
# stable-x86_64-pc-windows-gnu (...)
```

If `rustup override` is not what you want, prefix commands instead:
`cargo +stable-x86_64-pc-windows-gnu build --release -p bk-cli`.

### Common GNU-toolchain snags

| Error | Fix |
|---|---|
| `linker link.exe not found` | you're still on the msvc toolchain — `rustup override set stable-x86_64-pc-windows-gnu` |
| `gcc: command not found` | install the GNU *toolchain* (not just the target); it bundles gcc |
| `Could not find cmake` | `winget install Kitware.CMake`, reopen the terminal |
| `nasm: command not found` | `winget install NASM.NASM`, reopen the terminal |
| `error: failed to run custom build command for aws-lc-sys` | CMake + NASM missing (above), or your gcc is 32-bit — install 64-bit MinGW |

---

## Option C — build with MSVC (Visual Studio Build Tools)

If you prefer the default Windows toolchain, install the C++ workload:

```powershell
winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --quiet --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

Then `cargo build --release -p bk-cli` works with the plain `stable` toolchain.

---

## About Vulkan

Vulkan is **not needed**. Earlier designs of this project used a GPU training backend
(wgpu/Burn); the shipped version trains on the CPU in pure Rust and does inference with
AVX2/AVX-VNNI integer kernels. Having Vulkan installed does no harm, but nothing uses it.

---

## Training models on Windows

The prebuilt Windows binary already includes the trainer.

```powershell
# option A: download the prebuilt models (no training)
.\bookmark.exe model install

# option B: train locally (seconds to a couple of minutes)
.\bookmark.exe model train all --mode quick
.\bookmark.exe model verify

# restart the server so it loads them
.\bookmark.exe serve
```

To train in the background while you keep working:

```powershell
Start-Process -WindowStyle Hidden -FilePath .\bookmark.exe -ArgumentList "model","train","all","--mode","full"
# or, to watch it:
Start-Job { .\bookmark.exe model train all --mode full }
Get-Job | Receive-Job -Wait
```

Models are written to `%USERPROFILE%\.local\share\bookmark\models\`. Once present, the
search page no longer shows the **degraded** badge.

---

## Running it in the background on Windows

`bookmark service install` explains the Task Scheduler path. The short version:

```powershell
schtasks /Create /SC ONLOGON /TN bookmark /TR "C:\path\to\bookmark.exe watch"
```

`bookmark watch` is the supervisor: it restarts the server with backoff if it exits.

---

## Where your data lives

```
%USERPROFILE%\.local\share\bookmark\
├── bookmark.db        SQLite database
├── bookmark.log
└── models\            *.bkmdl
```

Config: `%USERPROFILE%\.config\bookmark\config.toml`.
Set `BOOKMARK_CONFIG` to use a different config file.
