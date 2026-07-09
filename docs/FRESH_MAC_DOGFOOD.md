# Fresh Mac Dogfood

Use this on a separate Mac before calling GLIMPS public-beta ready. The point is
simple: behave like a new user, not like the person who already knows every
shortcut in this repo.

This flow does not install GLIMPS globally and does not edit that Mac's
`~/.zshrc`.

## Setup

Requirements:

- macOS
- Rust toolchain
- zsh
- optional: `cargo-audit`

Apple Silicon and Intel Macs use the same commands. Cargo builds a native binary
for whichever Mac is running the checkout.

Clone the repo and run:

```bash
scripts/dogfood-macos.sh check
```

That builds and runs the repo-local checks. It does not install GLIMPS.

## Start A Disposable Session

Run:

```bash
scripts/dogfood-macos.sh session
```

This starts `target/debug/glimps` with a temporary `ZDOTDIR`, temporary
`.glimpsrc`, and temporary zsh config. It does not append anything to
`~/.zshrc`, does not call `cargo install`, and cleans up the temporary directory
after the session exits. It preserves your real `HOME`, so Git credential
helpers and desktop applications continue using their normal user configuration.

Inside the session, check the normal happy path:

- command headers appear above output;
- JSON is badged and pretty-printed;
- log, HTTP, diff, and stack-trace coloring are readable;
- ordinary commands still feel ordinary.

Then check the trust path:

- `vim`, `less`, `man`, `ssh`, `tmux`, `fzf`, `top`, and `htop` are not
  corrupted;
- binary or control-byte output is not framed;
- `exit`, Ctrl-D, Ctrl-C, terminal resize, and SIGTERM behave normally.

Do not skip the boring commands. They are where terminal wrappers lose trust.

## Terminal Matrix

Repeat `scripts/dogfood-macos.sh session` on:

- an Apple Silicon Mac;
- an Intel Mac;
- Terminal.app;
- iTerm2;
- Ghostty.

Also repeat with common zsh setups:

- plain zsh;
- Oh My Zsh;
- Starship;
- Powerlevel10k;
- zsh-autosuggestions;
- zsh-syntax-highlighting.

Record failures in `docs/LAUNCH_HARDENING_CHECKLIST.md` before public beta. A
known bug in the checklist is better than a surprise in a stranger's terminal.
