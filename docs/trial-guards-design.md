# Trial guards: design and dependency

## What the fetched work provides

The task branch includes `b465557` and `49361f3`, plus their ancestors. A bounded board restores its controller and request-gate environment from a durable binding. It asks the controller for a role lease, passes that lease to Codex with mandatory sandbox options, and refuses unsupported agent programs. It carries the binding through native recovery and permits the isolated reports directory. The earlier plan-sandbox commits keep a plan writer's deferred completion alive and settle it outside the read-only mount.

The controller's existing request gate checks the approved model and tier, measures input before a request, bounds output, and reserves money. The board's ordinary `limit.tokens` is different: it counts saved conversations after a turn, so one turn can exceed the limit. Neither is a hard aggregate token ceiling. The board selects planner, worker and reviewer models separately; its launch lease does not check the selected model. A trial's initial planning session is outside the board and must be brought under the same guard by its launcher.

## Smallest sound design

A trial starts with one durable, immutable run identity, token ceiling and exact model. The board must bind every planner, worker, reviewer and repair launch to that identity, supply the pinned model explicitly, and refuse a different program or model, a missing or changed binding, and any fallback. Continuations and recovery must retain that binding. The pipeline launcher must bind the initial planning and handoff sessions to the same run before their first request; a board cannot guard a process it did not start.

The shared request controller must atomically reserve measured input plus a provider-enforced maximum output against the run's remaining tokens **before each request**, including cached input, across all roles and concurrent sessions. It must persist reservations and usage across restarts, keep uncertain requests reserved, and refuse on missing accounting, changed model, exhausted budget or controller failure. The existing request gate must enforce the output bound on the actual provider request. A turn-end watcher, a per-task counter, or a dollar allowance cannot establish this guarantee. Provider behavior and the supported runtime's request paths remain part of the trust boundary.

## Pipeline-side change required

In `eric-os-build`, the run launcher and budget controller must accept and persist the token ceiling and model at run creation, atomically enforce token reservations and settlement alongside the existing dollar reservations, and launch the external planner and handoff sessions with the same mandatory lease and gate. They must reject legacy or unbound paths. This board task does not change that repository. Until those changes and the board integration are tested together, no trial has the requested hard cap or all-role pin.

## Verification needed

Use isolated fixtures and no model calls. Exercise all roles, continuation and restart, model mismatch and fallback, concurrent reservations, cached input, exact boundary, exhausted budget, missing or uncertain usage, and controller failure. A test of the board alone cannot prove that an external planner or the provider request obeys the shared cap. A supported runtime and pipeline integration test are required before a trial.
