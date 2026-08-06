# Demo

The README demo is generated from [`glimps.tape`](./glimps.tape) with
[VHS](https://github.com/charmbracelet/vhs). The point is to keep the demo
reproducible. If the product changes, the tape changes with it, and reviewers can
see what story we are telling.

## Render it

```bash
# 1. Install vhs (https://github.com/charmbracelet/vhs)
brew install vhs            # macOS
# or: go install github.com/charmbracelet/vhs@latest

# 2. Render with the repo-local binary (writes demo/glimps.gif)
scripts/render-demo.sh
```

The render script runs `cargo build --bin glimps`, puts `target/debug` at the
front of `PATH` for that process only, and then invokes VHS. The tape writes the
GLIMPS integration into a throwaway zsh config (`glimps init zsh > $TMP/.zshrc`)
and starts a wrapped shell, so it does **not** install GLIMPS globally, read or
modify your `~/.zshrc`, or change your login shell.

## Visual evidence checklist

Use this checklist before publishing or attaching demo media. The goal is to
show the beta honestly: what GLIMPS improves, what it leaves alone, and which
install paths are still being verified.

### Demo commands worth showing

The VHS tape should keep a short representative loop instead of trying to show
every formatter:

- `echo '{"login":"octocat","id":1,"admin":true,"plan":{"name":"pro","seats":10}}'`
  to show the command header and JSON formatting.
- `printf 'INFO  server up on :8080\nWARN  slow query 320ms\nERROR connection reset by peer\n'`
  to show streaming log severity coloring.
- `ls -la` to show ordinary command output passing through while still getting a
  clear command boundary.

If you capture a manual before/after, run the same commands in a plain zsh
session first, then run them in the repo-local GLIMPS session started by
`scripts/render-demo.sh` or `scripts/dogfood-macos.sh session`. Keep the
terminal size close to the tape settings (`1200x760`, 18px font) so the
comparison is about GLIMPS behavior, not a different viewport.

### Generated media location

- `demo/glimps.gif` is the reviewed release/README demo artifact. Commit it only
  after the visual has been checked in a terminal-sized viewport.
- `demo/*.draft.gif` is for local review drafts and is intentionally
  git-ignored.
- `demo/*.mp4` is for local video captures and is intentionally git-ignored.

Before opening a PR with demo media, confirm the ignore rules match the intended
artifact:

```bash
git check-ignore -v demo/review.draft.gif
git check-ignore -v demo/review.mp4
git check-ignore -q demo/glimps.gif || echo "demo/glimps.gif is intentionally trackable"
```

### Claims that must stay out of the demo

Do not claim any release channel that has not been verified from a real version
tag. Until the release/tap flow is complete, demo docs and captions must not say:

- `brew install glimps`
- `cargo install glimps`
- that a Homebrew tap, crates.io package, or release tag is ready for users

Use the current honest wording instead: repo-local dogfood, source install with
`cargo install --path .`, and beta release work still in progress.

## Wire it into the README

Once `demo/glimps.gif` exists and the visual has been reviewed on a real
terminal-sized viewport, replace the `<!-- DEMO: ... -->` placeholder near the
top of the root [`README.md`](../README.md) with:

```markdown
![GLIMPS in action](demo/glimps.gif)
```

## Tweaking

Edit `glimps.tape` to change the commands shown, timing (`Sleep`), size
(`Set Width/Height/FontSize`), or palette (`Set Theme`). See the
[VHS command reference](https://github.com/charmbracelet/vhs#vhs-command-reference).
A short, legible loop reads better than a long one. Show the moment GLIMPS earns
its place: command header, structured output, readable logs, and ordinary output
left alone.
