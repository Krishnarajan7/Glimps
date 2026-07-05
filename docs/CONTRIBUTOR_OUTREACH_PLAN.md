# GLIMPS — Contributor Outreach Plan

This is about **recruiting contributors**, not launching the product. It's the
quiet, steady work of being findable and welcoming so that someone who likes
GLIMPS can become someone who ships a PR to it.

Product launch copy (Show HN, Reddit launch posts, etc.) is deliberately **out of
scope here** — that's a separate step for when the demo GIF and a verified
one-line install exist. Nothing below is a hype post. It's plumbing and etiquette.

The whole goal, in one line: **make it easy for a star to become a PR.**

## Do these before inviting anyone

Contributors bounce off a repo that looks unfinished or unresponsive. Ground the
outreach on a repo that's ready:

- [ ] Labels created (`scripts/gh/setup-labels.sh`).
- [ ] Good-first issues created and the welcome issue pinned
      (`scripts/gh/create-issues.sh`).
- [ ] `README` has a visible Contributing call-out. (Done.)
- [ ] CI is green on `main`, and the badge (if any) is accurate.
- [ ] `CONTRIBUTING.md` answers "how do I set up and prove my change is safe."
- [ ] Repo has a clear description and topics set (see below).
- [ ] You can realistically reply to a new issue/PR within a few days.

If any of these is shaky, fix it before driving traffic. First impressions with
open-source contributors are unforgiving.

## GitHub-native discovery (highest leverage, lowest effort)

Most contributors find projects *through GitHub itself*. Make GLIMPS surface:

- **Repo topics.** Add: `rust`, `cli`, `terminal`, `pty`, `formatter`,
  `developer-tools`, `command-line`, `zsh`, `syntax-highlighting`,
  `good-first-issue`. These power GitHub topic pages and search.
  (`gh repo edit --add-topic rust --add-topic cli ...` once `gh` is set up.)
- **Enable Discussions.** A low-pressure place for "how would I approach X?"
  questions that shouldn't be issues yet. Lower barrier than opening an issue.
- **Pin the welcome issue.** (The create-issues script does this.) It's the first
  thing a curious visitor clicks.
- **Keep 4–6 good-first issues open at all times.** As contributors claim them,
  promote more from `docs/GOOD_FIRST_ISSUES.md` into real issues. An empty
  `good first issue` list tells a newcomer "nothing for me here."

## Good-first-issue aggregators (passive, evergreen)

These sites index repos that welcome new contributors. Submitting once brings a
slow, steady trickle of exactly the right people:

- **goodfirstissue.dev** — indexes issues labeled `good first issue`.
- **up-for-grabs.net** — submit via PR to their repo; lists projects by tag.
- **firsttimersonly.com** — philosophy + a feed of `first-timers-only` issues;
  consider adding a couple of extra-hand-held issues with that label.
- **This Week in Rust — "Call for Participation."** Each issue lists Rust projects
  seeking contributors. Submit a good-first issue link via a PR to their repo.
  This is the single best Rust-specific contributor channel, and it's not a hype
  post — it's literally a list of tasks.

## awesome-lists (passive discovery for users *and* contributors)

Being listed brings users, and users are where contributors come from:

- **awesome-rust** (CLI / terminal section).
- **awesome-cli-apps**, **terminals / terminal-tools** lists.
- Any "modern unix tools" / "CLI productivity" list.

Open a small, honest PR adding GLIMPS with a one-line, accurate description. Don't
overclaim install methods that aren't live yet.

## Community channels (participate, don't broadcast)

The rule everywhere: **join the conversation, disclose you're the author, follow
each community's self-promotion rules, and never paste the same text into two
places the same day.** Contributors respect a maintainer who shows up as a person.

- **r/rust.** Not a top-level promo. Use the recurring threads:
  - the weekly *"What's everyone working on this week?"* thread — mention GLIMPS
    and that there are scoped good-first issues.
  - answer PTY/terminal/formatter questions when they come up; link only when
    genuinely relevant.
- **Rust Community Discord** (`#showcase`, `#lang-tools`) and the **CLI/terminal**
  corners of dev Discords/Slacks you're already in. Share the repo where showcase
  is invited; then be around to answer questions.
- **Your own Discussions tab first.** You don't need a dedicated Discord until
  there's a small group of regulars. GitHub Discussions scales down to zero
  contributors gracefully; an empty Discord looks worse than none.
- **Mastodon / Bluesky Rust & CLI circles.** A plain "I'm looking for contributors
  on a small Rust terminal tool, here are the good-first issues" post is fine and
  on-topic. Save the demo-GIF showpiece for the product launch step.

## Turning a claim into a merged PR

Recruiting is half of it. Retention is the other half:

- **Respond fast.** A reply within 24–48h on a first-timer's issue or PR is the
  difference between a contributor and a ghost. Even "thanks, I'll look tonight."
- **Review kindly and concretely.** Point at the exact line, explain the *why*
  (usually a safety invariant), and offer the fix. Never make a first-timer guess.
- **Let them claim work.** When someone comments "I'll take this," assign it and
  say thanks. Don't let two people collide.
- **Say thank you in public.** Credit contributors in release notes / a
  CONTRIBUTORS section. People contribute again where they felt seen.
- **Offer to pair.** For a nervous first-timer, "want to hop on a call / I can
  walk you through the formatter path" converts extremely well.

## A light cadence

You don't need a heavy schedule. Something like:

- **Weekly:** triage new issues/PRs; make sure ≥4 good-first issues are open;
  reply to anything waiting on you.
- **Monthly:** promote 2–3 new issues from `docs/GOOD_FIRST_ISSUES.md`; check the
  aggregator listings still point at live issues; thank recent contributors.
- **As it grows:** consider Discussions categories, a CONTRIBUTORS file, and only
  then a chat community.

## What "working" looks like

Track the funnel, not vanity:

- good-first issues **claimed** (not just viewed);
- **first-time** contributors who opened a PR;
- median **time-to-first-response** on issues/PRs (aim: < 48h);
- PRs **merged** from external contributors.

Stars are nice. A merged PR from a stranger is the real signal that the pipeline
works.
