<!-- title: Start contributing to GLIMPS (good first issues live here) -->
<!-- labels: good first issue,help wanted,docs -->

Welcome. If you want to make GLIMPS a little better without diving into the
scary parts of the codebase, this is the place to start.

GLIMPS wraps your shell in a PTY and quietly reformats output it recognizes —
JSON, logs, HTTP, diffs, and so on. The most approachable way to contribute is
to teach it one more small, well-shaped output type, or to sharpen the docs and
demos. You do **not** need to touch the PTY supervisor, raw-mode handling, or the
OSC-133 scanner to make a real, visible difference.

### Pick something to work on

Browse the [`good first issue` label](https://github.com/Krishnarajan7/Glimps/labels/good%20first%20issue).
Each one is scoped to a single command or file type, tells you which files to
touch, what "done" looks like, and — just as important — what output your change
must **leave alone**.

Good starting points right now:

- Color `git diff --name-only` path lists
- Color `git stash list` output
- Color ripgrep / grep search results
- Color `tree` output
- Color `docker ps` / `kubectl get pods` tables
- Color Cargo build/test summaries

### Before you write code

Read these two — they're short and they're the whole philosophy:

- [`CONTRIBUTING.md`](https://github.com/Krishnarajan7/Glimps/blob/main/CONTRIBUTING.md)
- [`docs/FORMATTER_DESIGN_GUIDE.md`](https://github.com/Krishnarajan7/Glimps/blob/main/docs/FORMATTER_DESIGN_GUIDE.md)

The one rule that matters most: **when GLIMPS is unsure, it should get out of the
way.** A formatter that mangles normal output is worse than one that does
nothing. Every task here asks for both a "yes, format this" test and a "no,
leave this alone" test.

### Try it like a user

```bash
git clone https://github.com/Krishnarajan7/Glimps
cd Glimps
scripts/dogfood-macos.sh session
```

That builds the repo-local binary and drops you into a throwaway zsh wrapped by
GLIMPS. It does not install anything globally and does not edit your `~/.zshrc`;
`exit` cleans it up.

### How to claim one

Comment on the issue you want ("I'll take this") so we don't double up. Ask
questions freely — a ten-line question is cheaper than a two-hundred-line PR that
went the wrong direction. First PR nerves are normal; small and correct beats big
and clever every time.

Thanks for being here.
