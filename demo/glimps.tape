# GLIMPS demo tape — for VHS (https://github.com/charmbracelet/vhs)
#
# Render:   scripts/render-demo.sh    ->  produces demo/glimps.gif
# Needs:    `cargo`, `zsh`, and `vhs`. The script builds the repo-local binary
#           and places target/debug on PATH for the render process only.
#
# It is fully self-contained: it writes the GLIMPS integration into a throwaway
# zsh config and starts a wrapped shell, so it does NOT depend on your ~/.zshrc
# and won't touch it.

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
Hide
Type "export GDEMO=$(mktemp -d) && glimps init zsh > $GDEMO/.zshrc && ZDOTDIR=$GDEMO zsh"
Enter
Sleep 1.5s
Type "clear"
Enter
Sleep 500ms
Show

# --- 1) Find your input, and read your JSON ---
# GLIMPS marks where output begins with a colored command header, and pretty-
# prints the JSON it recognizes.
Type "echo '{\"login\":\"octocat\",\"id\":1,\"admin\":true,\"plan\":{\"name\":\"pro\",\"seats\":10}}'"
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
