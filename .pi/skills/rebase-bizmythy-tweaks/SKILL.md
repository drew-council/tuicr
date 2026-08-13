---
name: rebase-bizmythy-tweaks
description: Rebase the bizmythy-tweaks branch onto the latest upstream release as a minor-version update, reviewing each branch commit and removing commits made irrelevant by upstream.
disable-model-invocation: true
metadata:
  short-description: Rebase and prune bizmythy-tweaks
---

# Rebase `bizmythy-tweaks`

Rebase `bizmythy-tweaks` onto the latest stable upstream release. This should be a minor-version bump. Review every branch commit and remove commits that are no longer relevant after the upstream update.

## Rules

- Operate only on the `bizmythy-tweaks` branch. If it is not checked out, stop rather than switching automatically.
- Require a clean working tree and no Git operation in progress.
- Prefer the remote named `upstream`; verify it is the canonical repository before using it.
- Exclude prereleases unless explicitly requested.
- Confirm the target advances the minor version without changing the major version. Ask if the version scheme or intended target is unclear.
- Create a uniquely named backup branch at the original tip before rewriting history.
- Do not push or alter remotes unless explicitly requested.

## Workflow

1. Inspect the branch, status, remotes, tags, repository instructions, and available test commands.
2. Fetch the canonical upstream remote and its tags.
3. Identify the latest stable release and verify that it is the intended minor-version update from the branch's current base.
4. Determine the branch's old base and list every unique commit oldest-first.
5. Inspect each commit's intent and compare it with the target release:
   - keep it if its behavior is still needed;
   - adapt it if upstream changed the surrounding code;
   - drop it if upstream already provides equivalent behavior or removed the code it depended on.
6. If a commit's relevance is uncertain, ask the user rather than guessing.
7. Create the backup branch, then rebase only the branch commits:

   ```bash
   git rebase --onto <target-release> <old-base> bizmythy-tweaks
   ```

8. Resolve conflicts according to the original commit's intent and the target release's current design. Do not blindly choose `ours` or `theirs`.
9. Review the result with `git range-diff`, inspect the cumulative diff, run `git diff --check`, and run the repository's relevant tests and formatting checks.

## Report

Summarize:

- old and new release/base;
- backup branch name;
- kept or adapted commits;
- dropped commits and why;
- conflict resolutions;
- validation results;
- final Git status.
