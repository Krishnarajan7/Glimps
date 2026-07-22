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
- GitHub private vulnerability reporting is enabled and the `main` ruleset
  requires the repository's CI checks according to
  `docs/REPOSITORY_SETTINGS.md`.

## 1. Repo Preflight

Run this from the checkout:

```bash
scripts/release-readiness.sh --strict
```

Strict mode fails if release-only tools such as `cargo-audit` or `dist` are not
installed. When you are cutting a specific tag, also pass `--tag` so the script
verifies `Cargo.toml`'s version matches (see step 5):

```bash
scripts/release-readiness.sh --strict --tag v0.1.0-rc.1
```

For day-to-day contributor checks, this is enough:

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
- confirm the release environment/ruleset limits tag creation and publishing to
  maintainers.

## 5. Bump the version to match the tag

`cargo-dist` rejects a tag whose version does not equal a workspace package
version (`dist` errors with "no package with version X.Y.Z"). The repo ships at
`version = "0.0.1"` as a placeholder, so **before tagging** set `Cargo.toml`'s
`[package]` `version` to exactly the tag you are about to push (without the
leading `v`), and commit it:

```bash
# for the release candidate below, set version = "0.1.0-rc.1"; for the final
# public tag in step 7, set version = "0.1.0". SemVer strings must match exactly.
$EDITOR Cargo.toml         # edit [package] version
cargo build                # refresh Cargo.lock with the new version
git add Cargo.toml Cargo.lock
git commit -m "chore: release v0.1.0-rc.1"

# Preflight guards this for you — it fails if the version and tag disagree:
scripts/release-readiness.sh --strict --tag v0.1.0-rc.1
```

## 6. Test Tag

Use a release candidate first:

```bash
git tag -s v0.1.0-rc.1 -m "GLIMPS v0.1.0-rc.1"
git push origin v0.1.0-rc.1
```

Then verify:

- the release workflow passes;
- GitHub Release artifacts exist for all configured targets;
- the shell installer installs into the expected location;
- downloaded artifacts verify with
  `gh attestation verify <artifact> --repo Krishnarajan7/Glimps`;
- the Homebrew formula lands in the tap;
- `brew install Krishnarajan7/homebrew-tap/glimps` works on a clean Mac;
- uninstall instructions remove the binary and shell integration cleanly.

Do not promote a failed release candidate in the README. Fix it, tag another
candidate, and keep the public install path honest.

## 7. Public Tag

Only after the release candidate is boring. Bump the version again (step 5) to
the final `0.1.0`, commit, guard it, then tag:

```bash
$EDITOR Cargo.toml         # [package] version = "0.1.0"
cargo build
git add Cargo.toml Cargo.lock && git commit -m "chore: release v0.1.0"
scripts/release-readiness.sh --strict --tag v0.1.0
git tag -s v0.1.0 -m "GLIMPS v0.1.0"
git push origin v0.1.0
```

After the workflow publishes, update the README install section only with
commands that actually work from a clean machine.

> Note: GitHub Releases artifacts downloaded in a **browser** get the macOS
> quarantine xattr (Gatekeeper "unidentified developer") because the binaries are
> not notarized. The shell installer and Homebrew paths do not — steer users to
> those, or tell browser-downloaders to run `xattr -d com.apple.quarantine ./glimps`.

## 8. Rollback

If a release is bad:

- mark the GitHub Release as prerelease or remove it;
- delete the bad tag only if no users should depend on it;
- revert the Homebrew formula update in the tap;
- put the known issue and the safe install path in the next release notes.

Do not hide terminal-corruption, install, or uninstall issues. Those are trust
bugs, and trust bugs deserve daylight.
