# scripts/gh — GitHub contributor setup

One-time scripts to stand up the contributor pipeline: labels, good-first issues,
and a pinned welcome issue. Issue bodies live in
[`.github/good-first-issues/`](../../.github/good-first-issues/) — edit those, not
the scripts, to change issue content.

## Prerequisites

The [GitHub CLI](https://cli.github.com), authenticated for this repo:

```bash
brew install gh        # macOS
gh auth login          # follow the prompts; pick this repo's account
```

## Run it (order matters)

```bash
# 1. Create/update the label set (idempotent — safe to re-run).
scripts/gh/setup-labels.sh

# 2. See what issues would be created, without creating them.
scripts/gh/create-issues.sh --dry-run

# 3. Create the issues and pin the welcome one.
scripts/gh/create-issues.sh
```

Both scripts act on the repo of the current directory by default. To target a
specific repo, set `REPO`:

```bash
REPO=Krishnarajan7/Glimps scripts/gh/setup-labels.sh
```

## Notes

- **Idempotent.** `create-issues.sh` skips any issue whose exact title already
  exists, so re-running after adding a new file only creates the new one.
- **Adding a task.** Drop a new `NN-name.md` file in
  `.github/good-first-issues/` with the two metadata lines at the top:

  ```markdown
  <!-- title: Short human title -->
  <!-- labels: good first issue,formatter,safety -->

  Body goes here...
  ```

  Then re-run `create-issues.sh`. Make sure every label already exists (add it to
  `setup-labels.sh` and re-run that first).
- **Pinning.** GitHub allows up to 3 pinned issues. The welcome issue
  (`00-start-here.md`) is pinned automatically; if the API call can't pin it,
  the script tells you to pin it from the issue page.
