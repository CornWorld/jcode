# jcode MSYS2 native support (v0.2)

Goal: on Windows, jcode defaults to the **MSYS2 path system** when MSYS2 is
installed. The `bash` tool and client input panel run through MSYS2 bash
(`C:\msys64\usr\bin\bash.exe`) instead of the hardcoded `cmd.exe`, and jcode
uses `cygpath` to convert paths between native-Windows and MSYS2 forms
correctly.

## Why cygpath

jcode is a native Windows process, so `std::env::current_dir()` and the session
working directory are native paths such as `C:\msys64\home\cornw\jcode`. MSYS2
maps paths through a mount table: that same directory is `/home/cornw/jcode`,
**not** `/c/msys64/home/cornw/jcode`. Only `cygpath` knows the mount table, so
conversion must go through it to be correct; a naive string rewrite (`C:\` ->
`/c/`) would produce the wrong MSYS2 path for `/home` and other mounts.

## Facts from source

- `crates/jcode-app-core/src/tool/bash.rs` `build_shell_command()`
  - Windows: resolves the effective shell via `msys2::resolve_shell_command`,
    then `cmd.exe /D /S /C "<cmd>"` only when no MSYS2 bash is available.
  - non-Windows: `bash -c <cmd>` (unchanged).
- `crates/jcode-app-core/src/server/client_actions.rs:36`
  `build_input_shell_command()` (client input panel) uses the same resolution.
- `crates/jcode-base/src/config.rs` `ToolConfig.shell_command` — optional
  explicit override. When unset, MSYS2 bash is auto-detected.

## Design

### New module: `crates/jcode-base/src/msys2.rs`

- `is_msys2_env()` — detect MSYS2/Cygwin via `MSYSTEM` / `OSTYPE=cygwin`.
- `find_msys2_bash()` — locate MSYS2 bash under known roots (`C:\msys64`,
  `C:\msys2`, `MSYS2_ROOT`) or on `PATH`.
- `resolve_shell_command(configured)` — effective shell on Windows:
  explicit `shell_command` wins, else auto-detected MSYS2 bash, else `None`
  (caller falls back to `cmd.exe`). Non-Windows always returns `None`.
- `to_msys_path(&Path) -> Option<String>` — `cygpath -u`, with a best-effort
  `/drive/...` fallback when `cygpath` is unavailable.
- `to_windows_path(&str) -> Option<PathBuf>` — `cygpath -w`, with a best-effort
  fallback.

### bash tool + client input panel

Both call `resolve_shell_command`. When the resolved shell is MSYS2 bash the
command is invoked as `<bash> -lc <command>` (login shell so the MSYS2 profile
sets up PATH and environment). jcode also exports to every MSYS2 command:

- `JCODE_MSYS2=1` — marker that the command runs in MSYS2 mode.
- `JCODE_MSYS_CWD=<cygpath -u working_dir>` — the session working directory in
  its MSYS2 form (e.g. `/home/cornw/jcode`), so scripts can reliably reference
  the project root.

The bash `$PWD` is auto-derived by MSYS2 from the process cwd via the mount
table, so commands already operate in the MSYS2 world without extra work.

## MSYS2 path-conversion semantics (from official docs)

When MSYS2 bash launches a native Windows program, arguments that look like
Unix paths are auto-converted to Windows (`/foo` -> `C:/msys64/foo`). Control
vars (set inside MSYS2, not globally):

- `MSYS2_ARG_CONV_EXCL` — exclude arg conversion (`*` or `;`-separated
  prefixes).
- `MSYS2_ENV_CONV_EXCL` — exclude env-var path conversion.
- `cygpath -u/-w/-m` — manual conversion.

Since jcode passes the whole command as one argv to `bash -lc`, the conversion
runs inside bash's own launch of child tools, which is exactly what we want
(commands keep working for both Windows-native and MSYS2 tools).

## Verification

1. `cargo build --release` succeeds with the change applied.
2. Run the built exe on Windows (under MSYS2 or from cmd):
   - `bash` tool `echo $0` prints `bash` (auto-detected, no config needed).
   - `pwd` prints the MSYS2 form of the working dir (e.g. `/home/cornw/jcode`).
   - `echo $JCODE_MSYS2` prints `1`; `echo $JCODE_MSYS_CWD` prints the MSYS2 cwd.
   - `which git` resolves inside MSYS2.
3. Set `[tools] shell_command` explicitly to force a shell; the value wins over
   auto-detection.
