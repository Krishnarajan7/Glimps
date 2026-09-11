# Windows support — spike status

> **Status: experimental, unverified on real hardware.** The crate compiles and
> lints cleanly for `x86_64-pc-windows-msvc`, the pure formatter test suite is
> platform-independent, and a PowerShell integration snippet exists. Nobody has
> yet run a GLIMPS session on a Windows machine. This page is the checklist for
> doing that, and for deciding whether Windows can be supported at all.

## What is already done

| Area | State |
|---|---|
| PTY layer | `portable-pty` picks its ConPTY backend on Windows 10 1809+. No GLIMPS code changes needed. |
| Raw mode | `crossterm` raw mode, plus `ENABLE_VIRTUAL_TERMINAL_INPUT` on stdin and `ENABLE_VIRTUAL_TERMINAL_PROCESSING` on stdout (`src/terminal.rs`). Original console modes are restored exactly on exit and on panic. |
| Signals | `SIGHUP`/`SIGWINCH` do not exist on Windows. Termination uses `SIGINT`/`SIGTERM`; resize is detected by polling the console size on the supervisor's existing 40 ms tick (`src/pty.rs`). |
| Shell defaults | No `SHELL` on Windows: `pwsh` if on `PATH`, else `powershell`. `HOME` falls back to `USERPROFILE`. `pwsh`/`powershell` get `-NoLogo` instead of `-i`. |
| Command names | `mysql.exe` and `C:\...\mysql.exe` resolve to the same command views as `mysql`. |
| Shell integration | `glimps init pwsh` prints a PowerShell snippet that mirrors the zsh marker contract (OSC 133 C/D/A, private OSC 7337/7338/7339, metadata records). Works on pwsh 7 and Windows PowerShell 5.1 syntax. **Untested.** |
| Formatter | `src/format/` has no platform code. The SQL/mysql views, JSON, logs, etc. are byte-for-byte the same on every platform. |
| CI | `windows-2022` runs fmt, clippy, tests, and bench build. Marked `continue-on-error` until it has been green for a while. `.gitattributes` forces LF so the golden corpus survives a Windows checkout. |

## The open question: is ConPTY a byte pipe?

Everything GLIMPS does rests on one assumption: **the PTY master hands back the
child's bytes verbatim.** On Unix that is true. ConPTY is different: it keeps a
virtual screen buffer and re-synthesizes VT output from it. Depending on the
conhost build it may

1. inject cursor-positioning and attribute sequences into plain output (which
   trips safety invariant #3: never reformat output that already has ANSI);
2. hard-wrap long lines at the console width and emit them as separate rows
   (breaking JSON detect-by-parse and wide SQL result tables);
3. drop or delay the OSC 133 markers emitted by the shell integration.

Newer conhost builds (the one bundled with Windows Terminal, and the in-box
one on Windows 11 since 2024) are much better at forwarding sequences than
older ones. This has to be measured, not assumed.

## Step 1 — build

On the Windows laptop, with the Rust toolchain installed (`rustup` default
`stable-x86_64-pc-windows-msvc`, which needs the Visual Studio Build Tools
"Desktop development with C++" workload):

```powershell
git clone https://github.com/Krishnarajan7/Glimps
cd Glimps
cargo test --all
cargo build --release
```

`cargo test --all` should be green: the PTY integration tests are Unix-only and
skip; everything else is pure.

## Step 2 — run the byte probe (the actual spike)

`examples/pty_probe.rs` opens a PTY through the same `portable-pty` layer GLIMPS
uses, runs a command in it, and prints every byte the master returned, escaped,
with a summary. Run it inside **Windows Terminal** first, then in a legacy
`conhost` window (Win+R, `cmd`), and compare.

```powershell
# 1. A plain one-line JSON value, narrow console. Does it come back as one
#    line of text, or wrapped into 40-column rows with cursor moves?
cargo run --example pty_probe -- --cols 40 pwsh -NoLogo -Command "'{""a"":1,""name"":""a fairly long json line that exceeds forty columns"",""n"":[1,2,3]}'"

# 2. The shell integration markers. Do the OSC 133 / 7337 sequences survive?
cargo run --example pty_probe -- --send "glimps init pwsh | Out-String | Invoke-Expression\r" --send "echo hi\r" --send "exit\r" pwsh -NoLogo
#    (set $env:GLIMPS_ACTIVE=1 first so the snippet takes its 'inside' branch)

# 3. A real SQL table wider than the console.
cargo run --example pty_probe -- --cols 60 pwsh -NoLogo -Command "mysql -u root -e 'SELECT * FROM information_schema.tables LIMIT 3'"
```

For reference, the **macOS baseline** of test 1 (`zsh -f` at 40 columns) is:

```
cursor moves (CSI H/C/D): 0
OSC 133 markers:          (as emitted)
longest visible line:     79 cells (pty is 40 cols)
```

Read the verdict like this:

| Probe result | Meaning | Consequence |
|---|---|---|
| 0 cursor moves, line longer than the console width, markers intact | ConPTY is passing bytes through | Full port is feasible; go to step 3. |
| Lines wrapped at console width, cursor moves present | ConPTY is re-rendering | The formatter would need a VT-parsing screen-reconstruction layer in front of it. That breaks the one-seam byte-pipe model. Support Windows via WSL only, and say so. |
| Markers missing | conhost strips unknown OSC | Same as above; no boundary detection possible. Check whether Windows Terminal's bundled conhost behaves differently from the in-box one. |

## Step 3 — try a real session

Only if step 2 passed:

```powershell
# Session with an explicit shell (no profile edits yet)
.\target\release\glimps.exe --shell pwsh
# inside it, once:
glimps init pwsh | Out-String | Invoke-Expression
# then:
mysql -u root -e "SHOW DATABASES"
echo '{"a":1}'
glimps doctor
```

Things to check, in order of how bad they are if broken:

1. **Exit restores the console.** Type `exit`; the outer PowerShell must accept
   input normally, arrow keys must work, and colors must still render. This is
   safety invariant #1.
2. **Arrow keys, Ctrl+C, Tab completion** work inside the session (VT input).
3. **Window resize** propagates (drag the window; `$Host.UI.RawUI.WindowSize`
   inside the session should follow).
4. **`GLIMPS=0`** (`$env:GLIMPS='0'`) gives pure pass-through.
5. Interactive programs (`vim`, `ssh`, `less`) are bypassed by name. If they
   are not, the metadata channel is not being written: check that
   `$env:GLIMPS_META_PATH` was captured (`$Global:__glimps_meta_path` inside
   the session) and that appends succeed (a sharing violation would be
   swallowed silently).
6. **`exit` from the profile ends the outer host.** After the GLIMPS session
   ends you must be back at the Windows Terminal tab, not in a second bare
   PowerShell. If you are, the `exit $LASTEXITCODE` in the snippet only aborted
   the profile; switch it to `[Environment]::Exit($LASTEXITCODE)`.
7. **Legacy code-page output.** Run a tool that prints non-ASCII in the OEM
   code page (`chcp 437; dir` with accented file names). ConPTY should hand
   GLIMPS UTF-8; if a session's output ever goes silent after such a command,
   the console stdout rejected a non-UTF-8 byte and the reader loop stopped.
8. `Set-StrictMode` users: the wrapped `prompt` runs with strict mode off for
   its own `$?`/`$Error` inspection, and child scopes inherit that.

To install permanently, add near the **top** of `$PROFILE` (`notepad $PROFILE`):

```powershell
if (Get-Command glimps -ErrorAction SilentlyContinue) { glimps init pwsh | Out-String | Invoke-Expression }
```

PowerShell has no `exec`, so the outer shell waits for the GLIMPS session and
exits with its status. A Windows Terminal profile with
`"commandline": "glimps.exe --shell pwsh"` avoids the double startup.

## Known limits and non-goals

- `cmd.exe` has no hooks of any kind and will only ever get pass-through.
- `glimps setup` does not edit `$PROFILE` (Documents may be OneDrive-redirected
  and only the shell knows the real path). It prints the line to add instead.
- `glimps doctor` reports the *conventional* profile path, which may be wrong
  on a redirected Documents folder.
- WSL is Linux and already works with the zsh/bash integration. It is the
  zero-effort fallback if ConPTY turns out not to pass bytes through.
