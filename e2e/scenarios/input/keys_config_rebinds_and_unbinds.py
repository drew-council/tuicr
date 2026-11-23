#!/usr/bin/env python3
# config.toml `[keys.<context>]`: a key or `<leader>` sequence bound to an action runs it, `"none"` removes a default
# binding, and a binding that displaces a longer default warns at startup. (#983)
from e2e import Scenario, repos, scenario

CONFIG = """\
[keys.normal]
"x" = "help"
"?" = "none"
"<leader>x" = "toggle-file-list"
"<leader>e" = "none"
"d" = "half-page-down"
"""

HELP = "Esc to close"


@scenario
def main(s: Scenario) -> None:
    repo = repos.git_repo(s.dir / "repo")
    repos.write(s.dir / "config" / "tuicr" / "config.toml", CONFIG)
    tui = s.start("-r", "HEAD~2..HEAD", cwd=repo, keep=True)
    tui.wait_for("Files · 0/8")
    tui.wait_for("keys.normal 'd' replaces 'dd'")  # `d` can no longer start `dd`
    tui.keys("Escape")
    tui.keys("?")  # unbound: nothing opens
    tui.keys(";x").wait_gone("Files · 0/8")  # the configured leader sequence hides the list
    tui.keys(";x").wait_for("Files · 0/8")
    tui.reject(HELP)
    tui.keys("x").wait_for(HELP)  # the configured key opens help
    tui.keys("Escape").wait_gone(HELP)
    tui.quit()
