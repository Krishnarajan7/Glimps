<!-- title: Color `kubectl get pods` status output -->
<!-- labels: good first issue,formatter,command-aware,safety -->

`kubectl get pods` is the quick "is my cluster okay?" view. Coloring the STATUS
column — green for `Running`, warning/error colors for the bad states — lets you
spot a `CrashLoopBackOff` in a long list without reading every row.

Start with pods only. The other Kubernetes tables can come later.

### What to build

Command-aware coloring for `kubectl get pods`: pod name, READY, STATUS,
RESTARTS, AGE. The STATUS coloring is the point.

### Where to look

- `src/format/mod.rs`
- `src/format/linefmt.rs`
- `src/format/tests.rs`

### Done when

- The header row is distinct.
- `Running` is green.
- `Pending`, `CrashLoopBackOff`, `Error`, and `ImagePullBackOff` are warning or
  error colored.
- Non-pod `kubectl` output (e.g. `kubectl config current-context`) passes through.

### Keep it safe

Scope this to `kubectl get pods` only for the first PR. Don't try to support
every Kubernetes resource table at once — that's how the shape check gets loose
and starts coloring things it shouldn't.

### Prove it

- A test with one `Running` pod.
- A test with one failing pod.
- A rejection test for `kubectl config current-context` output.

---

**New to GLIMPS?** Read [CONTRIBUTING.md](https://github.com/Krishnarajan7/Glimps/blob/main/CONTRIBUTING.md)
and the [Formatter Design Guide](https://github.com/Krishnarajan7/Glimps/blob/main/docs/FORMATTER_DESIGN_GUIDE.md).
To try your change like a real user, run `scripts/dogfood-macos.sh session` — it
wraps a throwaway zsh and cleans up on exit, without touching your `~/.zshrc`.
Comment here to claim it, and ask anything before you start.
