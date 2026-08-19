# GLIMPS failure-intelligence tape — for VHS (https://github.com/charmbracelet/vhs)
#
# Render:   scripts/render-demo.sh    ->  produces demo/failure.gif (and glimps.gif)
# Needs:    `cargo`, `zsh`, `python3`, and `vhs`. The script builds the repo-local
#           binary and places target/debug on PATH for the render process only.
#
# Companion to glimps.tape. That one shows GLIMPS formatting output it recognizes;
# this one shows the thing no byte-stream tool can do, because GLIMPS owns the PTY
# and reads OSC-133 markers: it knows the command -> output -> exit code -> duration
# boundary. See docs/FAILURE_INTELLIGENCE_PLAN.md.
#
# Everything shown here is decoded from what the shell already reported. There is
# no AI, no guessing, and no network. The failing test is a real `python3 -m
# unittest` run, not a scripted echo.
#
# Like glimps.tape, this runs in a seeded throwaway project with a plain prompt so
# no local files, usernames, or hostnames reach the frames.
#
# NOTE on quoting: VHS does not support backslash-escaped quotes inside a
# double-quoted `Type "..."` argument. Use backticks for any line containing
# quotes.

Output demo/failure.gif

Require glimps

Set Shell zsh
Set FontSize 18
Set Width 1200
Set Height 800
Set Padding 20
Set TypingSpeed 55ms
Set Theme "Catppuccin Mocha"

# --- Setup (hidden) ---
# The project lives at a short, fixed path on purpose: Python prints absolute
# paths in tracebacks, and a `mktemp -d` path would put a random temp hash (and
# on some systems a username) into the frames.
Hide
Type `export GCFG=$(mktemp -d)`
Enter
Type `glimps init zsh > $GCFG/.zshrc`
Enter
Type `print 'PROMPT="%1~ %# "' >> $GCFG/.zshrc`
Enter
Type `mkdir -p /tmp/glimps-demo && cd /tmp/glimps-demo`
Enter

# A module with a real bug: the rate is 0.2, so charge(10) returns 2.0, not 5.
Type `printf 'def charge(amount):\n    return amount * 0.2\n' > billing.py`
Enter
Type `printf 'import unittest\nfrom billing import charge\n\nclass TestBilling(unittest.TestCase):\n    def test_charge(self):\n        self.assertEqual(charge(10), 5)\n' > test_billing.py`
Enter

Type `ZDOTDIR=$GCFG zsh`
Enter
Sleep 1.5s
Type "clear"
Enter
Sleep 500ms
Show

# --- 1) An exit code you don't have to look up ---
# 127 becomes "command not found on PATH", and the failed command is named.
Type `carg build`
Sleep 700ms
Enter
Sleep 2s

# --- 2) The failure a pipeline normally swallows ---
# `false | wc -l` exits 0, because the shell reports the LAST stage. GLIMPS reads
# the whole pipeline status array and says so.
Type `false | wc -l`
Sleep 700ms
Enter
Sleep 2.4s

# --- 3) Not everything non-zero is a failure ---
# Ctrl-C exits 130. GLIMPS calls it interrupted, in a notice color, not an error.
Type `sleep 5`
Sleep 500ms
Enter
Sleep 900ms
Ctrl+C
Sleep 2.2s

# --- 4) The one that saves the scrolling ---
# A real failing unittest run. The assertion is 5 lines above the footer, so
# GLIMPS pins it and attributes it to file:line.
Type `python3 -m unittest`
Sleep 700ms
Enter
Sleep 3.2s

Sleep 1.5s
