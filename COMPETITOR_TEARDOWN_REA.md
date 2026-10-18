# GLIMPS Competitor Teardown (REA)

Date: 2026-10-17 · Tool: REA (`rea-agents`) 6.5.0 · Companion to
[`COMPETITIVE_PRODUCT_GAP_ANALYSIS.md`](./COMPETITIVE_PRODUCT_GAP_ANALYSIS.md)

The earlier gap analysis compared GLIMPS with **open-source CLI tools** (bat,
delta, lnav, grc, fx…) using their READMEs. This document looks inside the two
**shipped terminal products** that overlap most with GLIMPS's core idea of
"command blocks with structure". Their behaviour was recovered from the
installers themselves rather than from marketing pages:

| Target | Version analysed | Build | How it was analysed |
|---|---|---|---|
| **Warp** (closed source) | `v0.2026.10.14.08.27.stable_01` | Rust, universal Mach-O (x86_64 + arm64), macOS ≥ 10.14 | REA plist + Mach-O + artifact inventory; strings from the x86_64 slice; bundled `settings_schema.json` |
| **Wave Terminal** | `v0.14.5` | Electron frontend + Go backend (`wavesrv`) + `wsh` helper | REA `analyze-javascript-application` on `app.asar`; strings from `wavesrv` |

Nothing was executed. Both installers were mounted read-only and inspected
statically.

---

## TL;DR: the five moves that matter

1. **Secret redaction in any terminal.** Warp ships a regex catalog that
   masks API keys, tokens, IPs and phone numbers in output, but it's **off by
   default** and **only works inside Warp**. GLIMPS sits in the byte stream of
   *every* terminal, so a `glimps` "screen-share-safe mode" would be a new,
   demo-able feature for streamers, pair programmers and conference speakers.
2. **Long-running-command notifications.** Warp notifies after **30 s** by
   default, and also on password prompts. GLIMPS already measures command
   duration, so emitting a desktop notification escape code (OSC 9 / OSC 777)
   from the footer is a small change with a large payoff, and it works in
   iTerm2, Ghostty, WezTerm and kitty.
3. **Harden the command marker.** Both competitors **encode** the command
   text before putting it in an escape sequence (Wave: base64 `cmd64`, Warp:
   hex). GLIMPS sends it raw in OSC 7337 (`src/init.rs:100`, `:196`). A command
   line containing a literal BEL/ESC byte can end the OSC early. This is cheap
   hardening that both reference implementations already chose.
4. **Context-aware header, especially for "danger" contexts.** Warp's prompt
   hook reports `git_branch`, `virtual_env`, `conda_env`, `kube_config`,
   `node_version`, user and host on every prompt. GLIMPS can show a coloured
   **"⚠ kube: prod"** or **"git: main"** chip in the header *only when it
   matters*. That's a safety feature people will screenshot.
5. **Don't chase their surface area; win on trust.** Warp has **230 settings**,
   bundles Sentry, and has telemetry and crash reporting **on by default**,
   plus AI agents, cloud drive and accounts. GLIMPS's pitch is the opposite:
   *zero config, zero network, your own terminal*. Every feature below has to
   fit that pitch.

---

## 1. What Warp actually ships

### 1.1 Shell-integration protocol (recovered from embedded scripts)

Warp embeds its zsh/bash/fish/PowerShell bootstrap scripts as plain text in the
binary. Escape sequences observed (occurrence counts in the x86_64 strings):

| Sequence | Purpose (inferred from surrounding script text) |
|---|---|
| `DCS $f … ST` (`\eP$f`) | Main channel: **hex-encoded JSON** "hook" messages (`warp_send_json_message`) |
| `OSC 9278;k / ;d / ;f` | Completion / generator plumbing |
| `OSC 9280;A/B/C/D/S` | Prompt / command-description markers (`D?description` carries hex) |
| `OSC 9277;A/B` | Additional prompt markers |
| `OSC 133;A/B/P` | Standard FinalTerm marks (also emitted for interoperability) |

**Hook types** (from the Rust enum and the scripts): `InitShell`, `Bootstrapped`,
`Precmd`, `Preexec`, `CommandFinished`, `InputBuffer`, `Clear`, `SSH`,
`PreInteractiveSSHSession`, `InitSubshell`, `SourcedRcFileForWarp`,
`FinishUpdate`, `ExitShell`, `ExternalShellWidgetSelection`.

**Context fields sent with hooks:** `pwd`, `git_branch`, `git_head`,
`virtual_env`, `conda_env`, `kube_config`, `node_version`, `user`, `host`,
`shell`, `shell_version`, `shell_plugins`, `vi_mode_enabled`, `aliases`,
`abbreviations`, `function_names`, `builtins`, `env_var_names`, `histfile`,
`rcfiles_start_time` / `rcfiles_end_time` (**shell startup timing**),
`os_category`, `linux_distribution`, `wsl_name`, `next_block_id`.

**Takeaways for GLIMPS**
- Encoding the payload (hex, base64) is standard practice. GLIMPS should do
  the same for 7337.
- Rich prompt context is cheap to collect in `precmd`, and it powers a lot of
  UI. GLIMPS's 7338 (cwd) channel could carry a small, opt-in context record.
- `rcfiles_start/end_time` means Warp can report **slow shell startup**. A
  `glimps doctor --startup` would be a cheap, popular diagnostic.

### 1.2 Feature surface (from the bundled `settings_schema.json`, 230 settings)

Settings relevant to the scrollback and output experience (AI, editor, tabs and
accounts are omitted):

| Warp setting | What it means | GLIMPS status |
|---|---|---|
| `privacy.secret_redaction.*` + `custom_secret_regex_list`; display modes: asterisks / grey strikethrough / visible | Mask secrets in output. **Default: off.** | **Gap → opportunity #1** |
| `notifications.preferences`: long-running (threshold **30 s**), password prompt, needs-attention, sound | Desktop notifications | **Gap → opportunity #2** (duration already measured) |
| `terminal.smart_select.enabled`: URLs, emails, file paths, identifiers | Clickable / selectable entities | Roadmap: OSC 8 hyperlinks → **raise priority** |
| `appearance.blocks.show_block_dividers`, `appearance.spacing` (Normal/Compact) | Block chrome | ☑ separator + header; compact mode is listed in the gap analysis |
| `appearance.blocks.show_jump_to_bottom_of_block_button` | Navigating long output | ☑ partly: error pinning "↑ 47 lines up" |
| Block actions: *Copy output*, *Copy command*, *Toggle block filter*, *Find within block*, *Bookmark*, *Share block*, *Scroll to top of block* | Per-block operations | **Gap**, can be emulated (see #6) |
| `warpify.subshells.added_subshell_commands` (regex) + denylist | Re-integrate inside `docker exec`, `poetry shell`, `nix-shell`, … | **Gap**: GLIMPS loses markers in subshells |
| `warpify.ssh.*` (install extension on remote, host denylist) | Blocks over SSH | GLIMPS bypasses `ssh` (correct default) |
| `terminal.input.command_corrections`, `error_underlining_enabled` | "Did you mean…" | ☑ partly: exit-code decode ("command not found on PATH") |
| `terminal.osc52_clipboard_access` (deny / write / read-write) | Clipboard via OSC 52 | Enabler for `glimps copy` (#6) |
| `appearance.text.enforce_minimum_contrast` | Readability on any theme | ☑ partly: `theme = "light"`; contrast guard is a gap |
| `appearance.themes.system_theme` + light/dark pair | Follow OS appearance | Gap: GLIMPS needs `theme = "auto"` (OSC 11 background query) |
| `privacy.telemetry_enabled` = **true**, `crash_reporting_enabled` = **true** | Data collection on by default; `Sentry.framework` is bundled | **GLIMPS advantage: no network, ever** |

### 1.3 Secret-redaction catalog (names observed next to their regexes)

`AWS Access ID`, `GitHub Classic Personal Access Token`, `GitHub OAuth Access
Token`, `OpenAI API Key`, `Anthropic API Key`, `Fireworks API Key`, `Generic SK
API Key`, `Google API Key` (`\bAIza[0-9A-Za-z-_]{35}\b`), `Slack App Token`,
`Stripe Key`, `IPv4 Address`, `IPv6 Address`, `Phone Number`, `MAC Address`.

GLIMPS already has the matching concept for *commands* (`sensitive_commands`,
`src/config.rs:70`). Redacting **output** is the natural next step.

---

## 2. What Wave Terminal actually ships

REA application graph (Evidence `ev_dd8acde8…abea6d`): 1 BrowserWindow, a 57-member
context-bridge API, 120 IPC operations (60 literal channels), 19 parsed JS
files, 0 parse failures. Integrity note: the declared ASAR hash for
`dist/bin/wavesrv.x64` differs from the shipped file, which is consistent with
codesigning after packing. It was recorded as untrusted rather than treated as
tampering.

**Shell integration (`OSC 16162`)**, from `wavesrv`'s embedded scripts:

| Marker | Payload |
|---|---|
| `16162;M` | `{"shell","shellversion","uname","integration"}` sent once per shell |
| `16162;C` | `{"cmd64": "<base64 command>"}`: **command is base64-encoded** |
| `16162;D` | `{"exitcode": N}` |
| `16162;I` | `{"inputempty": true/false}` input-buffer state |
| `16162;A` | prompt start |

**Per-block metadata keys** found in the frontend: `shell:lastcmd`,
`shell:lastcmdexitcode`, `shell:state`, `shell:integration`, `shell:omz`
(detects oh-my-zsh!), `cmd:cwd`, `cmd:runonstart`, `cmd:clearonstart`,
`cmd:persistent`; terminal settings including `term:durable` (sessions that
survive restarts), `term:trimtrailingwhitespace`, `term:bellindicator`,
`term:scrollback`.

**Takeaways for GLIMPS**
- Wave and Warp independently landed on *encode the command* and *send JSON
  metadata*. GLIMPS's private OSCs (7337/7338/7339) are fine, but the 7337
  payload should be encoded.
- `shell:omz` detection: Wave special-cases oh-my-zsh. GLIMPS's doctor
  already warns about coexisting integrations, so add an explicit OMZ / p10k
  compatibility check and test fixture.
- `term:trimtrailingwhitespace` on copy is a small polish idea for any
  future `glimps copy`.
- `term:durable` and `wsh` (open files and previews as panes) are app-level
  features. **Out of scope**; GLIMPS shouldn't try to be a terminal emulator.

---

## 3. Gap matrix

| Capability | Warp | Wave | GLIMPS today | Direction |
|---|---|---|---|---|
| Works inside the user's *existing* terminal | ✗ (is the terminal) | ✗ (is the terminal) | **✓** | Core moat; say it louder |
| No account / no telemetry by default | ✗ | n/a | **✓** | Core moat |
| Command header and block separation | ✓ | ✓ | ✓ | Parity |
| Structured formatting (JSON/HTML/logs/HTTP/diff/SQL tables…) | partial | ✗ | **✓ deep** | Lead; keep widening |
| Exit-code decode, error pinning, pipeline status | partial | exit code only | **✓** | Lead |
| Secret redaction in output | ✓ (opt-in) | ✗ | ✗ | **Build: opportunity #1** |
| Long-running / password-prompt notifications | ✓ (30 s) | bell indicator | ✗ | **Build: #2** |
| Encoded command marker | ✓ hex | ✓ base64 | ✗ raw | **Fix: #3** |
| Prompt context (git / venv / kube / node) | ✓ | partial | cwd only | **Build: #4** |
| Clickable URLs / paths (OSC 8) | ✓ | ✓ (xterm.js) | roadmap | **Build: #5** |
| Copy / filter / find within last output | ✓ | partial | ✗ | **Emulate: #6** |
| Subshell re-integration (`docker exec`, `nix-shell`…) | ✓ | ✗ | ✗ | Later: #8 |
| fish | ✓ | ✓ | ✗ | Roadmap; both competitors ship it |
| Shell startup profiling | ✓ | ✗ | ✗ | Cheap win: #7 |
| Auto light/dark theme | ✓ | ✓ | manual | Small: #9 |

---

## 4. Ranked opportunities

Each opportunity has to respect `docs/SAFETY_INVARIANTS.md`: it can be turned
off, it passes through when unsure, and it never corrupts bytes that are piped.

### #1 Secret redaction: "screen-share-safe mode" ★ flagship
- **What:** mask known secret shapes in *displayed* output: `sk-…` → `sk-•••••`.
  Ship Warp's catalog as a starting set (§1.3), and let users add patterns via
  `[redaction] patterns = [...]` in `.glimpsrc`.
- **Modes:** `off` (default) · `mask` · `dim` (grey strikethrough, like Warp).
  Toggle mid-session with `glimps redact on|off`, reusing the existing
  provenance-guarded OSC 7340 toggle path.
- **Why it can go viral:** "I streamed my terminal and leaked my OpenAI key" is
  a recurring story. GLIMPS fixes it **in Terminal.app, iTerm2, Ghostty, VS
  Code's terminal**, i.e. everywhere Warp's version can't reach.
- **Safety:** only active on a TTY, so piped output is untouched. Matches that
  are split across chunks are handled by the existing line buffering. Needs
  goldens plus false-positive tests (hashes, UUIDs, base64 blobs).
- **Effort:** M. **Good-first-issue spin-offs:** one pattern per issue.

### #2 Long-running-command and password-prompt notifications
- **What:** if a command ran longer than `notify_after = "30s"` and the terminal
  is not focused, emit `OSC 9;<msg>` (iTerm2/WezTerm/Ghostty) or
  `OSC 777;notify;GLIMPS;<msg>` (kitty/foot/urxvt), with a fallback to BEL.
  Message: `✓ cargo build: 2m14s` or `✗ npm test: exit 1 (3 failed)`.
- **Detecting focus:** request focus reporting (`CSI ?1004h`) only when it is
  safe to do so. Otherwise notify every time and leave it to the user's
  threshold setting.
- **Password prompt:** GLIMPS already has a password-prompt test, so it could
  reuse that detection to notify "waiting for input".
- **Effort:** S. Duration and exit code already exist in the footer
  (`src/format/mod.rs:856`, `:936`).

### #3 Encode the OSC 7337 command payload (hardening)
- **What:** emit `7337;b64:<base64>` from zsh, bash and pwsh, and accept both
  the encoded and legacy forms during one release.
- **Why:** both competitors encode. A literal control byte in a command line
  (rare, but possible via paste or Ctrl-V) currently ends the OSC early.
  *Inferred from code reading; not yet reproduced. Write the failing test
  first.*
- **Effort:** S. Ideal good first issue with a clear acceptance test.

### #4 Context chips in the header, loudest when risky
- **What:** `precmd` adds an opt-in context record (git branch, venv,
  kube context, AWS profile). The header shows it **only when relevant**: when
  the command is `kubectl`/`helm`/`terraform`/`aws`, or when the context
  matches a user `danger = ["prod", "production"]` list, render a red
  `⚠ kube: prod-eu` chip.
- **Why:** it extends the "failure intelligence" story into "*prevent* the
  failure". It also makes a great screenshot.
- **Effort:** M. It needs a new private OSC (or an extension of 7338) and a
  per-shell implementation.

### #5 OSC 8 hyperlinks for URLs, file paths and `file:line`
- Already on the roadmap. Warp's `smart_select` shows that users value it.
  Make `src/main.rs:42:7` in compiler errors and stack traces clickable
  (`file://` plus the editor scheme), since GLIMPS already parses those
  formats. **Effort:** M.

### #6 "Last block" commands (emulating Warp's block actions)
- `glimps copy`: copy the previous command's output to the clipboard via
  OSC 52, after trimming trailing whitespace (as Wave does).
- `glimps last | grep foo`: re-emit the last output *raw* for piping.
- `glimps find <pattern>`: show only matching lines of the last output, with
  context lines.
- **Constraint:** GLIMPS would need to keep a bounded copy of the last output
  in memory only, never on disk. Off by default if retention is a privacy
  concern. **Effort:** M.

### #7 `glimps doctor --startup`: shell startup profiling
- Time from shell spawn to the first `133;A` prompt marker, which GLIMPS
  already observes in the PTY. Report it, and suggest `zprof` when it exceeds
  300 ms. Warp collects this (`rcfiles_start/end_time`). **Effort:** S.

### #8 Subshell re-integration
- Detect `docker exec -it`, `kubectl exec -it`, `poetry shell`, `nix-shell`
  and `su`, and show a one-line hint (`glimps: inside docker exec, formatting
  paused`) instead of silently losing markers. Warp's approach is a regex list
  plus denylist (`warpify.subshells.*`). **Effort:** S for the hint, L for
  full re-integration.

### #9 `theme = "auto"`
- Query the background colour with `OSC 11 ?` at startup and pick the
  dark/light palette. Add a minimum-contrast guard (Warp's
  `enforce_minimum_contrast`). **Effort:** S–M.

### #10 fish integration
- Both competitors ship fish integration. Warp uses
  `--on-event fish_preexec/fish_prompt`; Wave sends the same `16162` markers.
  The GLIMPS marker contract maps one-to-one. **Effort:** M.

### Don't build
AI agents, cloud sync, accounts, a block UI with a custom renderer, durable
sessions, a remote SSH agent. Those are what make Warp and Wave *terminal
apps*. GLIMPS's growth story is "**the upgrade that works in the terminal you
already love**".

---

## 5. Making GLIMPS "the next big thing on GitHub"

Product moves (from above), packaged as launch moments:

| Launch beat | Hook line | Built on |
|---|---|---|
| v0.2 "Stream-safe" | *"Your terminal now hides your API keys, in any terminal."* | #1 |
| v0.3 "Ping me" | *"Walk away from `cargo build`. GLIMPS taps you when it's done."* | #2 + #7 |
| v0.4 "Don't nuke prod" | *"A red ⚠ before `kubectl delete` on prod."* | #4 |
| v0.5 "Click the error" | *"Every `file:line` in every stack trace is clickable."* | #5 |

Each beat ships with a 10-second GIF (rendered from `scripts/render-demo.sh`
tapes), a before/after image, and 3–5 good-first-issues cut from the feature
(one redaction pattern, one notification backend, one context provider…).

---

## 6. Evidence ledger

| Claim | Evidence | Type |
|---|---|---|
| Warp bundle identity and Info.plist (URL scheme `warp`, "New Warp Tab Here" service) | `ev_454fcd197f2d4948ba78da03375f52d39ecb8834527edc5639eeab55457123db` | observed (REA inspect_plist) |
| Warp app inventory | `ev_0c2395daa55c207e7bfd0629d6801a35093512b640cc0431f9273b5d656a8d33` | observed (REA inspect_artifact) |
| Warp Mach-O: universal x86_64+arm64, min macOS 10.14, links Metal/MetalKit, UserNotifications, **Sentry.framework** | `ev_5c5f30292520f9407e4960e5361fe671236b97118550f959d496640d3ddbb5c7` | observed (REA inspect_macho) |
| Warp settings, defaults (telemetry on, crash reports on, redaction off, 30 s notify) | `Warp.app/Contents/Resources/settings_schema.json` | observed (shipped file) |
| Warp hooks, OSC/DCS protocol, context fields, secret catalog | `strings` of x86_64 slice of `Contents/MacOS/stable` (sha256 `876425ff…a633`) | observed text; *purpose of each OSC is inferred* |
| Wave app graph and IPC counts | `ev_dd8acde84326a61bc1a91562623fa3a31293b5fc1fd6bc3f27077fad41abea6d` | observed (REA analyze_javascript_application) |
| Wave OSC 16162 payloads | `strings` of `app.asar.unpacked/dist/bin/wavesrv.x64` | observed text |
| GLIMPS sends raw command in 7337 | `src/init.rs:100`, `src/init.rs:196` | observed (source) |
| Raw 7337 can be truncated by a control byte | — | **inferred, unverified**; write a test |

Installer hashes: Warp.dmg `866204c9…5c1`, Wave.dmg `a26413d2…f42`.

### Unknowns and limits
- **No native decompilation.** Ghidra, Hopper and IDA aren't configured on this
  machine, so Warp's Rust logic (e.g. *how* it matches secrets across chunks,
  its notification focus logic) was not read. Only strings and shipped data
  were.
- Runtime behaviour of both apps was **not observed**. Nothing was launched.
- Purposes of Warp's private OSC numbers are inferred from adjacent script
  text.

---

## 7. Appendix: using REA efficiently (lessons from this run)

1. **Upgrade:** `npm install -g rea-agents@latest`, then refresh only Claude's
   skill with `rea setup --skill-client claude_code --dry-run`, then `--yes`.
   `--all-detected` would also write MCP config for Cursor, Gemini, VS Code and
   others.
2. **Restart the agent session after upgrading.** The MCP server already
   running keeps the old version in memory, while npm replaces its files on
   disk, and its calls then fail with `execution_failure`. The CLI (`rea …
   --format json`) runs the new version immediately.
3. **APFS DMGs:** `inspect_artifact` on Warp's DMG failed. Mount it yourself
   with `hdiutil attach -readonly -nobrowse -mountpoint <scratch>` and point
   REA at the `.app`.
4. **Electron apps:** a codesigned unpacked binary triggers
   `artifact_integrity_mismatch`. Rerun with
   `--integrity-policy record-and-continue`. Expect large output (255 MB for
   Wave), so query it with `jq` instead of reading it.
5. **Without Ghidra:** `inspect-plist`, `inspect-macho`, `inspect-artifact`,
   shipped JSON/YAML resources and `strings` still recover a lot, especially
   shell scripts, which apps embed as plain text. To get pseudocode, set
   `GHIDRA_INSTALL_DIR` to a Ghidra 12.1.x install.
6. **Long jobs:** run them in the foreground with a long timeout. Background
   `&` jobs die when the tool's shell exits.
