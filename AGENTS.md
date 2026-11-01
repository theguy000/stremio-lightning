# Working rules for agent workers

A worker crash on 2026-11-01 lost a full stage of work: the worktree was still at the base
commit because nothing had been committed. These rules bound that loss to a few minutes.

## Commit early, push often

1. Commit before starting each separate piece of the task (the navigation frame, then each page,
   then the tests, then the docs). A commit does not need to be final or passing.
2. Run the expensive check only at those commit points, not after every edit.
3. Push the branch after the first commit that compiles, so a replacement worker can see progress.
4. Never spend more than one step on a clean worktree.

When a supervisor restarts a crashed worker, it first looks for commits on the branch. If it finds
any, it resumes from the last one instead of starting over.

## Keep the running check cheap

`cargo check -p stremio-lightning --tests` on a cold target directory takes around ten minutes.
Run it once after a piece is done, not after every edit. When a check has been running for more
than 25 minutes, stop it, fix what the first errors say, and run it again.

## If you are at risk of running out of context

Commit everything that compiles, push, and write the remaining plan into the PR body, then stop and
report. A short, honest report plus a pushed branch is worth more than a perfect uncommitted tree.

## Report format

End with: PR URL, head SHA, CI conclusions, new files, contract changes, deviations and why.
