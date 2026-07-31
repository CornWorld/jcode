# jcode MSYS2 native support design (v0.1)

Goal: on Windows, allow the `bash` tool to execute through MSYS2 bash
(`C:\msys64\usr\bin\bash.exe`) instead of the hardcoded `cmd.exe`, via a
config option. Also document MSYS2 path-conversion behavior so Windows
paths passed inside bash commands keep working.

## Facts from source (v0.64.2, master 0b0ce09)

- `crates/jcode-app-core/src/tool/bash.rs:499` `build_shell_command()`
  - `#[cfg(windows)]` => `cmd.exe /D /S /C "<cmd>"`
  - `#[cfg(not(windows))]` => `bash -c <cmd>`
- `crates/jcode-app-core/src/server/client_actions.rs:36`
  `build_input_shell_command()` (client input panel):
  - windows => `cmd.exe /C <cmd>`; else => `bash -c <cmd>`
- `Config` (crates/jcode-base/src/config.rs:461) already has
  `pub tools: ToolConfig` (line 478), so adding a field there needs no new
  config section.
- `ToolConfig` derives `Default` + `#[serde(default)]` (line 561), so a new
  `Option<String>` field defaults to `None` with zero migration cost.
- Global accessor: `crate::config::config() -> &'static Config`
  (jcode-base/src/config.rs:254), usable from any tool code.

## Change set

### 1. crates/jcode-base/src/config.rs — ToolConfig

Add field:

```rust
/// Windows-only. Shell executable used by the `bash` tool and client
/// input panel when running commands. Defaults to `cmd.exe` (current
/// behavior). Set to e.g. `C:\\msys64\\usr\\bin\\bash.exe` to run
/// commands through MSYS2 bash instead. Ignored on non-Windows.
///
/// When set, the command is invoked as:
///   <shell_command> -lc <command>
/// (login shell so /etc/profile, PATH, and MSYS2 env init apply).
#[serde(default)]
pub shell_command: Option<String>,
```

`Option<String>` + derive Default => default `None`. `#[serde(default)]`
keeps old config files valid.

### 2. crates/jcode-app-core/src/tool/bash.rs — build_shell_command()

Windows branch becomes:

```rust
#[cfg(windows)]
{
    if let Some(shell) = crate::config::config().tools.shell_command.as_deref() {
        let mut cmd = TokioCommand::new(shell);
        cmd.arg("-lc").arg(cmd_str);
        cmd
    } else {
        // existing cmd.exe path
        let mut cmd = TokioCommand::new("cmd.exe");
        cmd.args(["/D", "/S", "/C"]).raw_arg(format!("\"{cmd_str}\""));
        cmd
    }
}
```

Notes:
- `-lc`: login shell so MSYS2 profile sets PATH (`/mingw64/bin`,
  `/usr/bin`) and any user .bashrc applies; `-c` would skip profile.
- `cmd_str` is passed as one argv, so MSYS2's automatic arg path
  conversion does not mangle it; the string content is the user's
  command and reaches bash verbatim.
- Existing non-windows branch unchanged (`bash -c`).

### 3. crates/jcode-app-core/src/server/client_actions.rs —
   build_input_shell_command()

Same pattern for the client input panel:

```rust
#[cfg(windows)]
{
    if let Some(shell) = crate::config::config().tools.shell_command.as_deref() {
        let mut cmd = Command::new(shell);
        cmd.arg("-lc").arg(command);
        cmd
    } else {
        let mut cmd = Command::new("cmd.exe");
        cmd.arg("/C").arg(command);
        cmd
    }
}
```

### 4. Docs (optional)

- Mention `[tools] shell_command` in config docs / README Windows section.

## MSYS2 path-conversion semantics (from official docs)

When MSYS2 bash launches a native Windows program, arguments that look
like Unix paths are auto-converted to Windows (`/foo` -> `C:/msys64/foo`).
Control vars (set inside MSYS2, not globally):

- `MSYS2_ARG_CONV_EXCL` — exclude arg conversion (`*` or `;`-separated
  prefixes).
- `MSYS2_ENV_CONV_EXCL` — exclude env-var path conversion.
- `cygpath -u/-w/-m` — manual conversion.

Since jcode passes the whole command as one argv to `bash -lc`, the
conversion runs inside bash's own launch of child tools, which is exactly
what we want (commands keep working for both Windows-native and MSYS2
tools).

## Verification plan

1. Baseline: `cargo build --release` succeeds with unmodified tree.
2. Apply patch; `cargo build --release` (incremental, small diff).
3. Run built exe with `[tools] shell_command` set to msys bash:
   - `bash` tool `echo $0` should print `bash` (not cmd).
   - `which git` should resolve inside msys.
   - `ls /d/byh/dance` should list project (msys path conversion).
4. Without the option (default), behavior unchanged (cmd.exe).
