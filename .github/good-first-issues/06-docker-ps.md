<!-- title: Color `docker ps` table output -->
<!-- labels: good first issue,formatter,command-aware,safety -->

`docker ps` prints a wide table: container id, image, command, created, status,
ports, names. It's the view people glance at constantly, and a bit of color —
especially green `Up` vs red `Exited` — turns "is everything running?" into a
one-second answer.

### What to build

Command-aware coloring for the default `docker ps` table: header row, container
id, image, status, ports, and name. Status coloring is the high-value part.

### Where to look

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`

### Done when

- The header row is dim or key-colored.
- Container id and name are easy to pick out.
- `Up ...` statuses are green; `Exited ...` statuses are red or yellow.
- If a row can't be split into columns confidently, it passes through.

### Keep it safe

Do **not** build a general whitespace-table formatter out of this. Keep it
command-aware (`docker ps`) and shape-checked. A generic "align columns on
whitespace" heuristic will eventually mangle something it shouldn't.

### Prove it

- A default `docker ps` output test.
- A `docker ps -a` test with an exited container.
- A rejection test for unrelated whitespace-aligned text.

---

**New to GLIMPS?** Read [CONTRIBUTING.md](https://github.com/Krishnarajan7/Glimps/blob/main/CONTRIBUTING.md)
and the [Formatter Design Guide](https://github.com/Krishnarajan7/Glimps/blob/main/docs/FORMATTER_DESIGN_GUIDE.md).
To try your change like a real user, run `scripts/dogfood-macos.sh session` — it
wraps a throwaway zsh and cleans up on exit, without touching your `~/.zshrc`.
Comment here to claim it, and ask anything before you start.
