# Compatibility And Known Issues

This page separates behavior exercised continuously in CI from combinations that
still need clean-machine confirmation. “Build/test verified” is not the same as
“manually dogfooded in every terminal.”

## Current Matrix

| Platform or shell | Status | Evidence or limit |
|---|---|---|
| macOS, zsh | Supported beta | PTY integration suite runs on pinned macOS CI; clean-machine terminal matrix remains in progress. |
| macOS, bash | Supported beta | Shell hooks and PTY behavior are tested; bash hook caveat below applies. |
| Linux x86_64, zsh | Supported beta | PTY integration suite runs on pinned Ubuntu CI. |
| Linux x86_64, bash | Supported beta | PTY integration suite runs on pinned Ubuntu CI. |
| macOS Intel | Build target; physical verification pending | Release configuration includes `x86_64-apple-darwin`. |
| Linux arm64 | Build target; physical verification pending | Release configuration includes `aarch64-unknown-linux-gnu`. |
| fish and other shells | Unsupported | They receive transparent pass-through when launched explicitly; shell integration is not provided. |
| Windows | Unsupported | GLIMPS currently depends on Unix PTY and signal behavior. |

Run `glimps doctor` after installation to inspect the binary, shell integration,
configuration, `PATH`, terminal state, and private metadata channel without
changing the machine.

## Terminal And Tool Coverage

The formatter contains conservative pass-through handling for alternate-screen
applications, binary output, password prompts, SSH, existing ANSI output, and
non-TTY output. The following manual matrix is still being completed:

- Terminal.app, iTerm2, Ghostty, and terminal multiplexers;
- Oh My Zsh, Starship, Powerlevel10k, zsh-autosuggestions, and
  zsh-syntax-highlighting;
- repeated lifecycle tests for resize, Ctrl-C, Ctrl-D, `exit`, and SIGTERM; and
- clean Apple Silicon and Intel machines.

See [FRESH_MAC_DOGFOOD.md](./FRESH_MAC_DOGFOOD.md) for the non-invasive test
procedure.

## Known Beta Issues

- Bash uses a `DEBUG` trap because bash has no native `preexec`. A tool that
  installs a raw replacement `DEBUG` trap after GLIMPS can disable command
  boundaries. Put the GLIMPS integration after that tool.
- Homebrew installation is not public until the tap and a real tagged release
  have been verified end to end.
- Mixed-content output, such as JSON embedded inside prose logs, is intentionally
  not reformatted yet.
- Unsupported shells do not receive formatting guarantees.

Please report terminal corruption, secret exposure, orphaned processes, or
release-integrity problems privately according to [SECURITY.md](../SECURITY.md).
