# GLIMPS demo tape — for VHS (https://github.com/charmbracelet/vhs)
#
# Render:   scripts/render-demo.sh    ->  produces demo/glimps.gif
# Needs:    `cargo`, `zsh`, and `vhs`. The script builds the repo-local binary
#           and places target/debug on PATH for the render process only.
#
# It is fully self-contained: it writes the GLIMPS integration into a throwaway
# zsh config and starts a wrapped shell, so it does NOT depend on your ~/.zshrc
# and won't touch it.
#
# The demo also runs inside a seeded throwaway project directory rather than this
# checkout. That keeps the capture reproducible on any machine and keeps local
# files, usernames, and hostnames out of the frames — see
# docs/VISUAL_EVIDENCE_CHECKLIST.md.
#
# NOTE on quoting: VHS does not support backslash-escaped quotes inside a
# double-quoted `Type "..."` argument. Use backticks for any line containing
# quotes.

Output demo/glimps.gif

Require glimps

Set Shell zsh
Set FontSize 18
Set Width 1200
Set Height 760
Set Padding 20
Set TypingSpeed 55ms
Set Theme "Catppuccin Mocha"

# --- Setup (hidden): enable GLIMPS in a temp config and enter a wrapped shell ---
# `glimps init zsh > .zshrc` writes the integration straight in (it's exactly what
# `eval "$(glimps init zsh)"` would run), so the new shell re-execs into GLIMPS.
#
# The prompt is reduced to `%1~ %# ` (real cwd basename, no color, no user@host).
# GLIMPS never touches PROMPT — see the doc comment in src/init.rs — so this is
# purely cosmetic, and it makes clear that every color in the frames is GLIMPS's.
Hide
Type `export GDEMO=$(mktemp -d)`
Enter
Type `glimps init zsh > $GDEMO/.zshrc`
Enter
Type `print 'PROMPT="%1~ %# "' >> $GDEMO/.zshrc`
Enter

# A small, believable project to run ordinary commands against.
Type `mkdir -p $GDEMO/acme-api/src && cd $GDEMO/acme-api`
Enter
Type `printf '# acme-api\n\nBilling and account service.\n' > README.md`
Enter
Type `printf '{\n  "name": "acme-api",\n  "version": "1.4.0"\n}\n' > package.json`
Enter
Type `printf 'node_modules/\ndist/\n' > .gitignore`
Enter
Type `printf 'export const port = 8080\n' > src/server.js`
Enter
Type `printf 'export function route(req) {}\n' > src/router.js`
Enter

Type `ZDOTDIR=$GDEMO zsh`
Enter
Sleep 1.5s
Type "clear"
Enter
Sleep 500ms
Show

# --- 1) Find your input, and read your JSON ---
# GLIMPS marks where output begins with a colored command header, and pretty-
# prints the JSON it recognizes.
Type `echo '{"login":"octocat","id":1,"admin":true,"plan":{"name":"pro","seats":10}}'`
Sleep 900ms
Enter
Sleep 2.2s

# --- 2) Logs get severity coloring as they stream ---
Type "printf 'INFO  server up on :8080\nWARN  slow query 320ms\nERROR connection reset by peer\n'"
Sleep 700ms
Enter
Sleep 2.2s

# --- 3) Ordinary output is untouched — but you can still find your command ---
Type "ls -la"
Sleep 700ms
Enter
Sleep 2.2s

Sleep 1.5s
