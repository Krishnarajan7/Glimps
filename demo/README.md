# Demo

The README demos are generated with [VHS](https://github.com/charmbracelet/vhs).
The point is to keep them reproducible. If the product changes, the tape changes
with it, and reviewers can see what story we are telling.

There are two, because GLIMPS makes two separate promises:

| Tape | Output | Shows |
|---|---|---|
| [`glimps.tape`](./glimps.tape) | `glimps.gif` | Formatting: the command header, JSON, log severity, and command-aware `ls` coloring |
| [`failure.tape`](./failure.tape) | `failure.gif` | Failure intelligence: exit-code translation, pipeline-stage warnings, Ctrl-C as a notice, and error pinning |

Both run in a seeded throwaway project with a plain `%1~ %# ` prompt, so no local
files, usernames, or hostnames reach the frames. GLIMPS never touches `PROMPT`
(see the doc comment in [`src/init.rs`](../src/init.rs)), so that is cosmetic
only — and it makes clear that every color in the frames came from GLIMPS.

## Render it

```bash
# 1. Install vhs (https://github.com/charmbracelet/vhs)
brew install vhs            # macOS
# or: go install github.com/charmbracelet/vhs@latest

# 2. Render with the repo-local binary (writes both GIFs)
scripts/render-demo.sh

# ...or just one
scripts/render-demo.sh demo/failure.tape
```

The render script runs `cargo build --bin glimps`, puts `target/debug` at the
front of `PATH` for that process only, and then invokes VHS. Each tape writes the
GLIMPS integration into a throwaway zsh config (`glimps init zsh > $TMP/.zshrc`)
and starts a wrapped shell, so it does **not** install GLIMPS globally, read or
modify your `~/.zshrc`, or change your login shell.

`failure.tape` additionally needs `python3` (stdlib only) for the real failing
`unittest` run, and it builds its fixture under `/tmp/glimps-demo` rather than a
`mktemp -d` path — Python prints absolute paths in tracebacks, and a temp path
would put a random hash into the frames.

## Wire it into the README

Both GIFs are already referenced from the root [`README.md`](../README.md):

```markdown
![GLIMPS in action](demo/glimps.gif)               <!-- near the top -->
![GLIMPS failure intelligence](demo/failure.gif)   <!-- "When something breaks" -->
```

Re-rendering overwrites those files in place, so no README change is needed to
refresh a demo — but do review the new render before committing it.

## Wire it into the website

The site's hero plays both captures as one video: formatting runs to the end,
failure intelligence follows, then the whole thing loops. It has to be video
rather than the GIFs, because the browser exposes no way to know when a GIF has
finished, so two `<img>` tags would just loop independently and out of sync.
Video also lets a visitor who asked for reduced motion get a still frame and
real controls instead — a GIF animates unconditionally.

After re-rendering the tapes, rebuild the reel (needs `ffmpeg`, not `vhs`):

```bash
# Join the two captures. glimps.gif is 1200x760 and failure.gif is 1200x800,
# so the shorter one is padded to match with the terminal background colour
# (#1d1c2d, sampled from the capture) rather than letterboxed in black.
ffmpeg -y \
  -i demo/glimps.gif -i demo/failure.gif -filter_complex \
  "[0:v]fps=25,scale=1200:760:flags=lanczos,pad=1200:800:0:20:color=0x1d1c2d,setsar=1[a];\
   [1:v]fps=25,scale=1200:800:flags=lanczos,setsar=1[b];\
   [a][b]concat=n=2:v=1:a=0[out]" \
  -map "[out]" -c:v libx264 -preset slow -crf 23 -pix_fmt yuv420p \
  -movflags +faststart site/public/demo.mp4

# Poster frame, shown before playback and to reduced-motion visitors.
ffmpeg -y -i site/public/demo.mp4 -frames:v 1 -q:v 3 site/public/demo-poster.jpg
```

H.264 only, deliberately: VP9 was no smaller here without a quality setting
that softened the terminal text, and h264 plays everywhere. Keep `crf` at 23 or
lower — this is small text, and it is the first thing to go when the bitrate
drops. Do not crop the canvas to the visible text: the tallest scene (the
pinned assertion at the end of `failure.tape`) fills almost the whole frame.

## Tweaking

Edit a tape to change the commands shown, timing (`Sleep`), size
(`Set Width/Height/FontSize`), or palette (`Set Theme`). See the
[VHS command reference](https://github.com/charmbracelet/vhs#vhs-command-reference).

Run `vhs validate <tape>` after any edit — it catches parse errors in seconds,
where a full render costs a rebuild. One gotcha worth knowing: VHS does **not**
accept backslash-escaped quotes inside a double-quoted argument, so
`Type "echo \"hi\""` is a parse error. Use backticks for any line containing
quotes: ``Type `echo "hi"` ``.

Keep `Set Height` just above what the content needs. Too tall wastes vertical
space and shrinks the text once GitHub scales the GIF to the README's width; too
short and the opening act scrolls off the top before the demo ends.

A short, legible loop reads better than a long one. Show the moment GLIMPS earns
its place: command header, structured output, readable logs — and, in
`failure.tape`, a real failure with a real exit code.
