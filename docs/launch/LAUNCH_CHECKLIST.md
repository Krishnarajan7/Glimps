# v0.1.0 Launch Checklist

The repo is launch-grade; this checklist turns it into an installable, announced
product. Order matters: **ship → verify → announce**. Never announce an install
path you haven't run on a clean machine (see
[`PUBLIC_BETA_RELEASE_RUNBOOK.md`](../PUBLIC_BETA_RELEASE_RUNBOOK.md)).

Ready-to-post copy for every venue: [`LAUNCH_COPY.md`](./LAUNCH_COPY.md).

## Phase 0 — One-time GitHub/account setup (maintainer, ~30 min)

- [ ] Create the tap repo: **github.com/Krishnarajan7/homebrew-tap** — public,
      empty (a README is fine). cargo-dist pushes `Formula/glimps.rb` into it.
- [ ] Create a fine-grained PAT with **Contents: read/write** on
      `Krishnarajan7/homebrew-tap` only, and add it to the Glimps repo as the
      **`HOMEBREW_TAP_TOKEN`** actions secret (used by
      `.github/workflows/release.yml`, job `publish-homebrew-formula`).
- [ ] Create a crates.io API token (crates.io → Account Settings → API Tokens,
      scope: publish-update) and add it as the **`CARGO_REGISTRY_TOKEN`**
      actions secret (used by job `publish-crate`).
- [ ] Repo settings → Environments: the `host` job uses environment
      **`release`**. GitHub creates it on first use; optionally pre-create it
      with yourself as required reviewer for a manual approval gate on releases.
- [ ] Repo settings → General: set the description to
      *"Zero-config smart terminal output formatter — your shell's output,
      structured and colored automatically. No piping."* and the website to
      `https://glimpps.netlify.app`.
- [ ] Repo settings → Topics: `terminal`, `cli`, `rust`, `pty`, `formatter`,
      `developer-tools`, `shell`, `zsh`, `json`, `command-line`.
- [ ] Repo settings → Social preview: upload `site/public/og.png`.
- [ ] Confirm Discussions is enabled and pin a "Roadmap / what's next" thread.
- [ ] File the specs in `.github/good-first-issues/` as real issues labeled
      `good first issue` (an empty tracker reads as a dead project).

## Phase 1 — Cut v0.1.0

- [ ] Land the release-prep changes on `main` (version `0.1.0`, `CHANGELOG.md`,
      README install paths, `publish-crate` job). **Note:** the README now
      advertises brew/installer/cargo paths — tag promptly after merging so
      they don't dangle.
- [ ] Sanity checks locally:
      `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test --all`
      and `cargo publish --dry-run --locked`.
- [ ] Tag and push: `git tag v0.1.0 && git push origin v0.1.0`.
- [ ] Watch the **Release** workflow: plan → build (4 targets) → host →
      publish-homebrew-formula + publish-crate → announce. All green.
- [ ] Release page shows 4 archives + `glimps-installer.sh` + checksums, and
      the release is **not** marked pre-release.

## Phase 2 — Verify before announcing (clean machine or fresh user account)

- [ ] `brew install Krishnarajan7/tap/glimps` → `glimps doctor` passes.
- [ ] `curl --proto '=https' --tlsv1.2 -LsSf https://github.com/Krishnarajan7/Glimps/releases/latest/download/glimps-installer.sh | sh`
      → binary on PATH, `glimps doctor` passes.
- [ ] `cargo install glimps` → works; crates.io page renders the README.
- [ ] The zsh one-liner from the README wraps a session; `GLIMPS=0 zsh` opts out.
- [ ] README badges all render green on the repo front page.

## Phase 3 — Announce (spread over ~2 weeks; copy in LAUNCH_COPY.md)

Each venue's feedback improves the next post, so don't fire them all in one day.
In every post, be explicit that **macOS + zsh is the primary path** and bash is
beta — bad first experiences travel faster than good ones.

- [ ] **Day 1 – Show HN** (news.ycombinator.com/submit). Tue–Thu, 8–10 am ET.
      Post the prepared first comment immediately after submitting. Stay
      available for ~4 hours to answer comments.
- [ ] **Day 2–3 – r/rust** (project post) and **r/commandline** (lead with the
      demo GIF, upload natively).
- [ ] **Day 3–5 – This Week in Rust**: PR adding GLIMPS to the next draft at
      github.com/rust-lang/this-week-in-rust (section: Project Updates /
      "Crate of the Week" nomination too).
- [ ] **Week 1 – Terminal Trove**: submit at terminaltrove.com (tool
      submission form).
- [ ] **Week 1 – Lobsters** (needs an invite; ask in your network or skip).
- [ ] **Week 2 – Awesome-list PRs**: `awesome-rust` (Applications →
      terminal), `awesome-cli-apps` (Productivity/Terminal), `awesome-terminals`
      / `awesome-zsh-plugins` where it fits. Slow burn, permanent discovery.
- [ ] **Week 2 – Technical blog post** ("How GLIMPS knows where your command's
      output begins — and why shell hooks can't do this"), cross-post to
      dev.to, link from the HN thread if it's still alive.
- [ ] **Ongoing – X/Mastodon** clips from `site/public/demo.mp4`, tag
      `#rustlang`.

## Phase 4 — The first two weeks after launch

- [ ] Answer every issue/comment fast — responsiveness is the #1 signal early
      adopters check before installing something that wraps their shell.
- [ ] Triage new bug reports into the existing issue templates; convert good
      feature asks into `formatter_request` issues.
- [ ] Ask early users in the pinned Discussion which formatter they want next.
- [ ] Cut a quick `v0.1.1` for any install-path papercut — a fast patch release
      one week after launch is itself a trust signal.
