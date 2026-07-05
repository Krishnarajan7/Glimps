# Public Beta Release Runbook

This is the maintainer path from "the repo looks good" to "a stranger can try
GLIMPS without private instructions." It is intentionally a little strict.
Terminal tools earn trust slowly and lose it fast.

The rule for releases: do not advertise an install command until it has worked
from a real release artifact on a clean machine.

## Ready Means

- Automated gates pass from the repo checkout.
- `cargo audit` is clean.
- Fresh-machine dogfood passes on Apple Silicon and Intel Mac.
- Terminal.app, iTerm2, and Ghostty are checked.
- `vim`, `less`, `ssh`, `tmux`, `fzf`, `top`, and `htop` pass through cleanly.
- `exit`, Ctrl-D, Ctrl-C, resize, and SIGTERM restore the terminal.
- The demo GIF is generated from `demo/glimps.tape` and reviewed.
- The Homebrew tap and release workflow are verified from a real tag.

## 1. Repo Preflight

Run this from the checkout:

```bash
scripts/release-readiness.sh --strict
```

Strict mode fails if release-only tools such as `cargo-audit` or `dist` are not
installed. For day-to-day contributor checks, this is enough:

```bash
scripts/release-readiness.sh
```

The script does not install GLIMPS globally, edit `~/.zshrc`, or start an
interactive GLIMPS session.

## 2. Fresh Mac Dogfood

On each Mac, clone the repo and run:

```bash
scripts/dogfood-macos.sh check
scripts/dogfood-macos.sh session
```

Cover both architectures before public beta:

- Apple Silicon Mac
- Intel Mac

Cover these terminal apps:

- Terminal.app
- iTerm2
- Ghostty

Use `docs/FRESH_MAC_DOGFOOD.md` as the detailed matrix. Record every failure in
`docs/LAUNCH_HARDENING_CHECKLIST.md` before tagging.

## 3. Demo GIF

Render the demo with the repo-local binary:

```bash
scripts/render-demo.sh
```

Review `demo/glimps.gif` before committing it. The demo should make the value
obvious in seconds: command header, JSON structure, log severity color, and
ordinary output staying normal.

## 4. Release Plumbing

Before pushing a public release tag:

- confirm `Krishnarajan7/homebrew-tap` exists;
- add `HOMEBREW_TAP_TOKEN` as a GitHub Actions secret with permission to push to
  that tap;
- confirm `dist-workspace.toml` still lists:
  - `aarch64-apple-darwin`
  - `x86_64-apple-darwin`
  - `aarch64-unknown-linux-gnu`
  - `x86_64-unknown-linux-gnu`
- confirm the README still says Homebrew is unavailable until the tap has been
  tested end-to-end.

## 5. Test Tag

Use a release candidate first:

```bash
git tag v0.1.0-rc.1
git push origin v0.1.0-rc.1
```

Then verify:

- the release workflow passes;
- GitHub Release artifacts exist for all configured targets;
- the shell installer installs into the expected location;
- the Homebrew formula lands in the tap;
- `brew install Krishnarajan7/homebrew-tap/glimps` works on a clean Mac;
- uninstall instructions remove the binary and shell integration cleanly.

Do not promote a failed release candidate in the README. Fix it, tag another
candidate, and keep the public install path honest.

## 6. Public Tag

Only after the release candidate is boring:

```bash
git tag v0.1.0
git push origin v0.1.0
```

After the workflow publishes, update the README install section only with
commands that actually work from a clean machine.

## 7. Rollback

If a release is bad:

- mark the GitHub Release as prerelease or remove it;
- delete the bad tag only if no users should depend on it;
- revert the Homebrew formula update in the tap;
- put the known issue and the safe install path in the next release notes.

Do not hide terminal-corruption, install, or uninstall issues. Those are trust
bugs, and trust bugs deserve daylight.
