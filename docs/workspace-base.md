# Starting a queued task from a linked checkout

For a new single-attempt task using `workspace.from: here`, delegation records the
caller checkout commit when it belongs to the task primary repository. The runner
verifies and uses this saved commit even if the caller moves while it waits.
Existing task worktrees resume in place. The primary checkout stays unchanged.
`workspace.from: main` keeps its existing behavior.
