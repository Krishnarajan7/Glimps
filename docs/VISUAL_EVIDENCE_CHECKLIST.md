# GLIMPS Visual Evidence Checklist

A terminal formatter is judged by a picture before it is judged by its code. That
makes screenshots and the demo GIF part of the product, and it makes them easy to
get wrong: a capture can promise an install path that does not exist yet, or show
a formatter behaving better than it does on a stranger's machine.

This checklist is the honest way to produce that evidence. It covers what to show,
how to capture before/after, where the files go, and what must not be claimed
while the release path is still being verified.

Use it whenever you produce a capture for the README, the website, an issue, a
pull request, or a launch post.

Legend: todo / in progress / done

---

## 1. Before You Capture

- Build the binary you are actually demonstrating: `cargo build --bin glimps`.
  Do not capture a stale binary from an earlier branch.
- Run from a checkout, not a global install, unless a global install is the thing
  being shown. `scripts/render-demo.sh` and `scripts/dogfood-macos.sh` both keep
  GLIMPS repo-local: they do not install it globally, edit `~/.zshrc`, or change
  your login shell.
- Use a clean terminal profile. A personal prompt theme, `git` status in the
  prompt, or an unrelated plugin's color makes it unclear which color GLIMPS
  produced.
- Clear scrollback first, so the capture starts at a known state.
- Check the window size. 1200x760 at font size 18 is what `demo/glimps.tape`
  uses, and it stays legible when GitHub scales the GIF down.
- Do not edit frames, retouch color, or splice takes. If a capture needs
  correction, fix the product or the tape and render again.

## 2. Commands Worth Showing

The demo has to earn its length. Show the moment GLIMPS changes something, then
stop. These are the commands the demo is built from, in the order that tells the
story:

- **The core fix — find your input.** Any command at all; the point is the `▌`
  header bar repeating the command above its output.
- **Structure — JSON.**
  `echo '{"login":"octocat","id":1,"admin":true,"plan":{"name":"pro","seats":10}}'`
- **Severity — logs.**
  `printf 'INFO  server up on :8080\nWARN  slow query 320ms\nERROR connection reset by peer\n'`
- **Restraint — ordinary output.** `ls -la`, shown deliberately to make the point
  that GLIMPS colors what it recognizes and leaves the rest alone.

Those four are required. `demo/glimps.tape` covers exactly this set; if you
change the tape, update this list so the two do not drift.

### The failure demo

Failure intelligence is the capability no byte-stream tool can copy, so it gets
its own capture (`demo/failure.tape` → `demo/failure.gif`). Its required beats:

- **A translated exit code.** `carg build` → `failed exit 127 — command not found
  on PATH`.
- **A pipeline failure the shell hides.** `false | wc -l` exits 0; GLIMPS warns
  that stage 1 failed.
- **A non-failure.** Ctrl-C during `sleep 5` → `interrupted exit 130 — Ctrl-C, not
  an error`, in notice color rather than red.
- **A pinned error.** A real failing `python3 -m unittest` run, where the
  assertion sits far enough above the footer to trip `MIN_PIN_LINES_UP` and get
  repeated with `file:line`.

Two rules specific to this capture. Use **real** failing commands — a scripted
`echo` that imitates a test runner is fabricated output, and this is the one
feature where a reviewer will check. And keep the failure ordinary: a typo, a
wrong constant, a pipeline mistake. A contrived catastrophe undersells a feature
whose whole point is the failure you hit every day.

Optional, when a capture is about one specific formatter rather than the product
as a whole — pick the one under discussion, not all of them:

- HTTP response splitting (status line, headers, cookies, body)
- unified diffs, or a Rust/Python stack trace
- `git status` / `git log`
- a command-aware view: `grep -n`, `cargo build`, `find`, `ls`, `df`, `dig`
- a CSV, SQL, JSON-lines, config, or source file through `cat`

Always show at least one **pass-through** case alongside a formatter case. The
safety story — "if GLIMPS cannot prove the shape, it does not touch the bytes" —
is as much of a selling point as the coloring, and it is the claim reviewers
distrust most. See `docs/SAFETY_INVARIANTS.md`.

Never capture real secrets. No live API tokens, private hostnames, internal URLs,
or customer data — even in a frame that scrolls past in half a second. Use
`octocat`, `example.com`, and obviously fake IDs.

## 3. Before/After Captures

"After" is a GLIMPS-wrapped shell. "Before" is the same command in the same
terminal with GLIMPS off, so the only variable is GLIMPS itself.

Get a raw shell one of two ways:

```bash
GLIMPS=0 zsh          # start a shell with GLIMPS disabled
```

If you are already inside a wrapped shell, run `exit` first, then start the raw
one. `GLIMPS=0` is the documented off-switch, so a before/after pair built this
way is reproducible by anyone reading the README.

- Run the **same command**, in the **same directory**, at the **same window
  size**, in both captures.
- Capture the before and after in the same session where possible. Font
  rendering and terminal padding change between apps and make an unfair
  comparison look like a doctored one.
- Label which is which. An unlabeled pair gets read in the wrong order.

## 4. Rendering The Demo GIFs

The README demos are generated, not hand-recorded, so that they can be
re-rendered when the product changes:

```bash
scripts/render-demo.sh                    # both tapes
scripts/render-demo.sh demo/failure.tape  # just one
```

The script builds the repo-local binary, puts `target/debug` at the front of
`PATH` for that process only, and renders each tape with VHS. It requires
`cargo`, `zsh`, `python3`, and `vhs`, and fails loudly if a render produced an
empty file. See `demo/README.md` for installing VHS and tweaking a tape.

Run `vhs validate <tape>` before a full render. VHS rejects backslash-escaped
quotes inside a double-quoted `Type "..."` argument, and a parse error costs a
rebuild to discover otherwise.

Review each result before committing it:

- The value is obvious within the first few seconds.
- Text is legible at GitHub's rendered width, not just at full size.
- No personal paths, hostnames, or secrets appear in any frame.
- Timing is readable — a viewer can finish each line before it scrolls.
- The loop does not end mid-command.

Both GIFs are already referenced from `README.md` — `demo/glimps.gif` near the
top and `demo/failure.gif` under "When something breaks" — and re-rendering
overwrites them in place, so refreshing a demo needs no README edit.

## 5. Where Generated Media Goes

- The reviewed README demos live at `demo/glimps.gif` and `demo/failure.gif` and
  **are** committed. They are deliberately not git-ignored: they are reviewed
  release artifacts, and the README is broken without them.
- Work-in-progress renders use the ignored patterns `demo/*.draft.gif` and
  `demo/*.mp4`. Name intermediate takes accordingly so they never land in a
  commit by accident.
- Website assets belong in `site/public/` — for example
  `site/public/demo-poster.svg`, the static poster the site uses in place of the
  animation.
- Source stays tracked: the tape, the render script, and this checklist are the
  reproducible part. Generated output is committed only when it has been
  reviewed.
- Before committing any media, confirm the ignore rules still behave:

  ```bash
  git status --short demo/
  git check-ignore -v demo/glimps.draft.gif   # expect a match
  git check-ignore -v demo/glimps.gif         # expect no match, exit 1
  ```

- Keep the GIF small enough to load on a slow connection. If it is getting
  heavy, cut a command rather than degrading the resolution people read the text
  at.

## 6. What Must Not Be Overclaimed

Every capture is a claim about what a stranger will get. Until the release and
tap flow is verified from a real version tag, these are not true yet, and must
not appear in a demo, screenshot, caption, or launch post:

- `brew install glimps` — the tap is configured but not verified end to end.
- `cargo install glimps` from crates.io — not published.
- fish shell integration — planned, not shipped.
- Any platform not marked verified in `docs/COMPATIBILITY.md`. Configured release
  targets are not the same as physically tested machines.

The supported install path today is `cargo install --path .` from a checkout, or
a repo-local dogfood session. Show that, or show no install step at all.

Also avoid the softer overclaims:

- Do not present a curated command set as though GLIMPS formats everything. It
  formats what it recognizes; the rest passes through by design.
- Do not describe bash support without the beta caveat in the README's
  "Known beta limits".
- Do not imply a stable release. The project is public beta, and the README says
  so — a capture should not contradict it.

When the tap and release flow are verified from a real tag, update the README,
`docs/COMPATIBILITY.md`, and this section together, in the same change. Stale
"not yet" text is its own kind of dishonesty.

## 7. Sign-Off

Before a capture ships anywhere public:

- todo: it was produced from a current build of the code it depicts;
- todo: it shows at least one formatted case and one pass-through case;
- todo: no secrets, private hostnames, or personal paths in any frame;
- todo: any before/after pair differs only by GLIMPS being on;
- todo: it makes no install claim from the list in section 6;
- todo: generated media is placed and ignored per section 5;
- todo: `git diff --check` is clean on the accompanying docs change.

Record failures and open questions in `docs/LAUNCH_HARDENING_CHECKLIST.md` rather
than fixing them silently in an image editor.
