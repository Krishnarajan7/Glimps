# GLIMPS Competitive Product Gap Analysis

Date: 2026-07-04

Goal: make GLIMPS feel like a product people can trust, not just a clever PTY
trick. The bet is simple: the best terminal tools are excellent when you
remember to call them. GLIMPS should bring that kind of clarity to the shell
session you were already using.

## Positioning

GLIMPS should not become a clone of `bat`, `delta`, `eza`, `fx`, `lnav`, `grc`,
or ChromaTerm. Those tools have their own jobs. GLIMPS wins only if it becomes
the automatic session layer:

- no command replacement required;
- no remembered pipe required;
- safe by default, with pass-through as the fallback;
- command-aware when the command is known;
- structurally formatted when the output type is known.

## Competitor Lessons

| Tool | What They Do Well | Gap GLIMPS Should Close |
|---|---|---|
| ChromaTerm | Whole-terminal coloring, interactive command wrapping, user regex rules. | Add user-extensible rule packs without losing GLIMPS's structural parsing advantage. |
| grc | Large command-config ecosystem for common Unix tools. | Build first-party command formatters for the commands users run daily. |
| bat | Syntax highlighting, Git/file context, man/help integration, broad install polish. | Add source-code/man/help formatters and make install/demo quality feel professional. |
| delta | Best-in-class diff/git UX: syntax highlighting, hunk navigation, side-by-side, grep/blame support. | Make Git/diff output feel premium, not just line-colored. |
| eza | Modern `ls`: file type colors, metadata, Git status, icons/themes, useful defaults. | Add command-aware table/path metadata coloring for `ls`, `find`, `du`, `df`. |
| lnav | Log format detection, time merging, error/warn indexing, JSON-lines support. | Improve log streams with timestamps, multiline grouping, and error summaries. |
| fx | Deep JSON viewing/processing and TUI navigation. | Add JSON folding/truncation/summaries while staying non-invasive in normal shell output. |
| rich-cli | High-quality rendering for syntax, Markdown, JSON, CSV, themes, guides. | Add richer semantic themes and more content types such as Markdown, CSV, YAML, SQL. |

Sources:

- ChromaTerm: https://github.com/hSaria/ChromaTerm
- grc: https://github.com/garabik/grc
- bat: https://github.com/sharkdp/bat
- delta: https://github.com/dandavison/delta
- eza: https://github.com/eza-community/eza
- lnav: https://github.com/tstack/lnav
- fx: https://github.com/antonmedv/fx
- rich-cli: https://github.com/Textualize/rich-cli

## Product Pillars

### 1. Command Awareness

The shell integration already captures the command and post-command cwd. That is
useful context, but it is not permission to color everything. Use it when the
command and the output shape agree:

- `cd`: show cwd breadcrumb after successful silent directory changes.
- `find`: color path segments and filenames.
- `ls`: color file types, permissions, sizes, dates, symlinks, Git hints where possible.
- `du` / `df`: align and heat-color sizes / usage percentages.
- `ps`: align CPU/memory columns, highlight high-resource rows.
- `dig` / `nslookup`: group question/answer/authority sections, color records.
- `curl`: status, headers, redirects, cookies, JSON/HTML body formatting.
- `git`: status, branch, and log output now color branch names, hashes, status
  codes, and file paths; deeper diff/stat polish remains open.

Rule: if a command formatter cannot prove the shape, it must pass through.
Looking helpful is not worth being wrong.

### 2. Content Format Depth

Current: GLIMPS handles JSON, HTML, logs, HTTP status lines, full HTTP
responses, diffs, stack traces, Markdown project files, YAML/TOML/INI/dotenv
config files, CSV/TSV tables, SQL query files, JSON-lines, common source files,
database result tables, and common Git output.

That is already enough to feel useful in real work. The next step is depth, not
random breadth.

Next:

- Broader source-code highlighting depth, including more languages, multiline
  parser state, and command output that is clearly code but lacks a filename.

Rule: source-code and Markdown should use a proven syntax/highlight library only
when the dependency and security story is acceptable. A heavy dependency is not
free just because it makes a demo pretty.

### 3. Premium Terminal UX

- More useful command header: command, cwd, elapsed time, exit code, output type.
- Failure summary when exit code is non-zero.
- TUI entry/exit breadcrumbs without mutating fullscreen app bytes.
- Optional compact mode for dense power-user terminals.
- Better palettes for light/dark terminals.
- No-color mode stays clean and structured.

### 4. Trust And Safety

- No telemetry, no logging terminal contents, no network runtime behavior.
- Always restore terminal state.
- Maintain byte-safety corpus for common commands.
- Keep binary and already-colored output conservative.
- Every formatter needs false-positive tests.

### 5. Contributor-Ready Growth

- Issue labels: `good first issue`, `formatter`, `command-aware`, `safety`,
  `docs`, `release`.
- One small command formatter spec per issue.
- Screenshots/GIF required for visual PRs.
- Keep `CONTRIBUTING.md` and safety invariants visible from README.

## Priority Roadmap

### P0: Public Beta Must Feel Complete

- Finish clean-machine macOS dogfood on Apple Silicon and Intel Mac.
- Verify release artifacts and Homebrew tap.
- Generate demo GIF.
- Add README examples for every shipped formatter.
- Add command-aware formatters for `ls`, `du`, `df`, `ps`, `dig`.
- Add man/help rendering that does not break pagers.

### P1: Make It Better Than Manual Pipes

- Richer full-patch diff polish beyond basic hunk/add/remove coloring.
- Broader source-code syntax depth for multiline/comment-heavy files.
- Clear-code output detection when no filename is available.

### P2: Make It Contributor-Magnetic

- Good-first-issue board with 10 small formatter tasks.
- Demo assets and before/after screenshots.
- Public formatter design guide.
- Plugin/rule-pack discussion document.
- Launch posts after Homebrew and demo GIF are verified.

### P3: Bigger Bets

- Optional folding/summaries for large JSON/HTML/log output.
- OSC 8 hyperlinks for URLs, paths, commits, and files.
- Bash/fish support.
- Local/offline optional AI summaries only after privacy and trust are boringly
  solid.
