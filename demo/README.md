# Demo

The animated demo for the project README is generated from [`glimps.tape`](./glimps.tape)
with [VHS](https://github.com/charmbracelet/vhs) — a tool that scripts a terminal
recording into a GIF, so the demo is reproducible and reviewable as a diff.

## Render it

```bash
# 1. Make sure glimps is installed and on your PATH
cargo install --path .

# 2. Install vhs (https://github.com/charmbracelet/vhs)
brew install vhs            # macOS
# or: go install github.com/charmbracelet/vhs@latest

# 3. Render (writes demo/glimps.gif)
vhs demo/glimps.tape
```

The tape is self-contained: it writes the GLIMPS integration into a throwaway zsh
config (`glimps init zsh > $TMP/.zshrc`) and starts a wrapped shell, so it does
**not** read or modify your `~/.zshrc`.

## Wire it into the README

Once `demo/glimps.gif` exists, replace the `<!-- DEMO: ... -->` placeholder near
the top of the root [`README.md`](../README.md) with:

```markdown
![GLIMPS in action](demo/glimps.gif)
```

## Tweaking

Edit `glimps.tape` to change the commands shown, timing (`Sleep`), size
(`Set Width/Height/FontSize`), or palette (`Set Theme`). See the
[VHS command reference](https://github.com/charmbracelet/vhs#vhs-command-reference).
A short, legible loop (JSON pretty-print → log coloring → ordinary output) reads
better than a long one.
