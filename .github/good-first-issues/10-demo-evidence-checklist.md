<!-- title: Write the demo/docs visual-evidence checklist -->
<!-- labels: good first issue,docs,release -->

A terminal formatter lives or dies on its demo. This task isn't code — it's
making GLIMPS easier to *show honestly*: a short checklist for producing
before/after terminal captures and the demo GIF from `demo/glimps.tape`.

Good task if you'd rather sharpen docs than write Rust for your first PR.

### What to build

A short, practical checklist covering: which commands to run for a representative
demo, how to capture before/after, where generated media should live, and what
must **not** be overclaimed while it's still being verified.

### Where to look

- `demo/README.md`
- `docs/LAUNCH_HARDENING_CHECKLIST.md`
- `README.md`

### Done when

- The checklist names the demo commands worth showing.
- It records what must not be overclaimed before Homebrew / crates.io / a real
  release tag is verified.
- It says where to place generated demo media.
- It keeps generated media git-ignored unless intentionally committed.

### Keep it safe

Docs must keep install claims honest. No `brew install glimps` or
`cargo install glimps` language until that path is actually verified from a real
version tag — the checklist should reinforce that, not undermine it.

### Prove it

- A read-through for clarity.
- `git diff --check` passes.
- Confirm `.gitignore` behaves as described for generated demo media.

---

**New to GLIMPS?** Read [CONTRIBUTING.md](https://github.com/Krishnarajan7/Glimps/blob/main/CONTRIBUTING.md).
Comment here to claim it, and ask anything before you start.
