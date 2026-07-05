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
