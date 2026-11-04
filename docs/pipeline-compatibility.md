# Legacy pipeline compatibility

This maintenance branch is based on the Rust herdr-projects 1.0.1 source. It is not the current Shepherdr layout,
and it does not migrate an installation to that layout. It contains only saved worker-end recovery, typed
completed-task ownership responses, and read-only discovery of those contracts.

`herdr-projects version --json` (also `capabilities --json`) prints a schema-1 object with `product: "herdr-projects"` and
`capabilities: {"worker_end_recovery": 1, "ownership_busy": 1}`. Discovery runs before startup migrations,
settings changes, task reads or native calls. `capabilities` also prints JSON without `--json`. Plain `version`
keeps its human-readable output. Callers probing an older board must use `version --json`: the old board
returns non-JSON version text, which can be refused. Set `HERDR_PROJECTS_INLINE=1` for this discovery
subprocess only to disable the old board’s startup migration routines. Never pass this override to real
task commands. The new discovery entrypoints bypass migrations themselves.

`worker_end_recovery: 1` means a saved, unhandled worker-end event can restart the normal finish path when the
worker is at rest and no finish runner lives, including when Herdr supplies no completion counter. Human waits,
reviewer events and other side sessions do not finish the worker.

`ownership_busy: 1` means report-done and archive recheck completed-task native ownership under the task lock.
A verified live owner returns exit 1 and `error.code: "ownership_busy"` on stderr with `--json`, without changing
the task. A caller may wait within its existing limits and retry. Active unfinished work, generic lock errors,
invalid requests and other failures remain distinct; callers must not infer temporary ownership from prose.

A version or checkout commit alone does not prove which executable is running. Pipeline callers must probe the
same executable they will use for task commands, then keep their tested-version check as a separate gate.
