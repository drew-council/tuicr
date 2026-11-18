#!/usr/bin/env python3
# `[review_guide] command` runs a user-supplied program when a PR opens: the request is JSON on stdin (instructions,
# PR metadata, per-file patches), the prompt is also in $TUICR_REVIEW_GUIDE_PROMPT, and the JSON guide on stdout
# reorders the files and adds a "Review guide" section to the PR panel. The guide is cached per PR head, and a
# failing command leaves the default order with a warning.
import json
import sys
from pathlib import Path

from e2e import Scenario, repos, scenario

GUIDE = {
    "overview": "- Adds `greet()` in `src/greet.rs`.\n- README gains a usage line.",
    "steps": [
        {"title": "Greeting function", "summary": "New public greet.", "files": ["src/greet.rs"]},
        {"title": "Docs", "summary": "", "files": ["README.md"]},
    ],
}

# Records what tuicr sent, waits for the scenario's go-ahead, then answers the way a chatty harness would:
# prose and a code fence around the JSON.
ANSWERING = """\
import os, pathlib, sys, time
d = pathlib.Path(sys.argv[1])
(d / "guide.request.json").write_text(sys.stdin.read())
(d / "guide.prompt.txt").write_text(os.environ.get("TUICR_REVIEW_GUIDE_PROMPT", ""))
with open(d / "guide.calls", "a") as f:
    f.write("call\\n")
while not (d / "guide.go").exists():
    time.sleep(0.05)
print("Here is the plan:\\n```json\\n" + (d / "guide.response.json").read_text() + "\\n```")
"""

FAILING = """\
import sys
sys.stdin.read()
print("progress noise", file=sys.stderr)
print("no credentials for the model", file=sys.stderr)
sys.exit(3)
"""


def configure(d: Path, script: str) -> None:
    d.mkdir(parents=True, exist_ok=True)
    path = repos.write(d / "guide.py", script)
    command = json.dumps([sys.executable, str(path), str(d)])
    repos.write(
        d / "config/tuicr/config.toml", f'[review_guide]\ncommand = {command}\nprompt = "Greeting code first."\n'
    )


def row_of(screen: str, needle: str) -> int:
    rows = [i for i, line in enumerate(screen.splitlines()) if needle in line]
    assert rows, f"{needle!r} is not on screen\n{screen}"
    return rows[0]


@scenario
def main(s: Scenario) -> None:
    d = s.dir
    configure(d, ANSWERING)
    repos.write(d / "guide.response.json", json.dumps(GUIDE))

    # The PR opens in directory order while the command runs, and says the guide is on its way.
    tui = s.pr_open(keep=True)
    tui.wait_for("generating review guide")
    before = tui.screen()
    assert row_of(before, "README.md [M]") < row_of(before, "src/greet.rs [A]"), before
    assert "Review guide" not in before, before

    request = json.loads((d / "guide.request.json").read_text())
    instructions = request["instructions"]
    assert instructions.startswith("Greeting code first.\n\n## Response format"), instructions
    assert (d / "guide.prompt.txt").read_text() == instructions, "prompt env differs from the request's instructions"
    assert request["pull_request"]["title"] == "Add greeting and tweak README", request["pull_request"]
    assert request["pull_request"]["head_sha"] == "2" * 40, request["pull_request"]
    assert [f["path"] for f in request["files"]] == ["README.md", "src/greet.rs"], request["files"]
    assert "+pub fn greet()" in request["files"][1]["patch"], request["files"][1]

    # The answer lands: files follow the guide's steps and the panel gains the section.
    repos.write(d / "guide.go", "")
    tui.wait_for("═══ Review guide").wait_gone("generating review guide")
    tui.expect(r"1\. Greeting function · New public greet\.").expect(r"2\. Docs")
    tui.expect(r"- Adds `greet\(\)` in `src/greet\.rs`\.")
    after = tui.screen()
    assert row_of(after, "src/greet.rs [A]") < row_of(after, "README.md [M]"), after
    assert row_of(after, "A greet.rs") < row_of(after, "M README.md"), f"file tree not reordered\n{after}"
    assert row_of(after, "More description text here") < row_of(after, "═══ Review guide"), after
    assert row_of(after, "═══ Review guide") < row_of(after, "PR #1 Status"), after
    assert "**" not in after, f"raw markdown markup on screen\n{after}"
    tui.quit()

    # Same head again: answered from the cache, the command does not run.
    tui = s.pr_open(keep=True)
    tui.wait_for("═══ Review guide").expect(r"1\. Greeting function")
    tui.quit()
    calls = (d / "guide.calls").read_text().splitlines()
    assert calls == ["call"], f"command ran {len(calls)} times for one PR head"

    # A failing command: default order stays, the last stderr line is the reason.
    failing = s.path_of(f"{s.name}_fail")
    configure(failing, FAILING)
    tui = s.pr_open(keep=True, name=f"{s.name}_fail")
    tui.wait_for("Review guide unavailable: .*no credentials for the model")
    screen = tui.screen()
    assert row_of(screen, "README.md [M]") < row_of(screen, "src/greet.rs [A]"), screen
    assert "═══ Review guide" not in screen, screen
    tui.quit()
