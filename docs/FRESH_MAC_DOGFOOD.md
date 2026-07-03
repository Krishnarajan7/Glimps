# Fresh Mac Dogfood

Use this on the separate Mac before calling GLIMPS public-beta ready. The point
is to test the real terminal experience without installing GLIMPS globally or
editing that machine's `~/.zshrc`.

## Setup

Requirements:

- macOS
- Rust toolchain
- zsh
- optional: `cargo-audit`

Clone the repo and run:

```bash
scripts/dogfood-macos.sh check
```

That builds and runs the repo-local quality gates. It does not install GLIMPS.

## Interactive Session

Run:

```bash
scripts/dogfood-macos.sh session
```

This starts `target/debug/glimps` with a temporary `ZDOTDIR`, temporary
`.glimpsrc`, and temporary zsh config. It does not append anything to
`~/.zshrc`, does not call `cargo install`, and cleans up the temporary directory
after the session exits.

Inside the session, verify:

- command headers appear above output;
- JSON is badged and pretty-printed;
- log, HTTP, diff, and stack-trace coloring look readable;
- ordinary commands still pass through;
- `vim`, `less`, `man`, `ssh`, `tmux`, `fzf`, `top`, and `htop` do not get
  corrupted;
- binary/control-byte output is not framed;
- `exit`, Ctrl-D, Ctrl-C, terminal resize, and SIGTERM behave normally.

## Terminal Matrix

Repeat `scripts/dogfood-macos.sh session` in:

- Terminal.app
- iTerm2
- Ghostty

Also repeat with common zsh setups on that Mac:

- plain zsh
- Oh My Zsh
- Starship
- Powerlevel10k
- zsh-autosuggestions
- zsh-syntax-highlighting

Record failures in `docs/LAUNCH_HARDENING_CHECKLIST.md` before public beta.
