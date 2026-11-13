# Trial guard contract, schema 1

This is the board/controller interface for an isolated bounded trial. It is not an installed feature or a claim that the external planner is guarded. EOB-18 implements the controller and launcher side.

## Binding

At run creation the launcher writes `pipeline-budget.json` in the isolated board data directory before any role starts. It retains the existing absolute `approval`, `database`, `control`, `gate`, `config` and `reports` fields and adds `trial`:

```json
{"schema":1,"run_id":"run-1","token_ceiling":1500000,"model":"gpt-6.1-sol"}
```

`run_id` is 1–128 ASCII letters, digits or hyphens; the ceiling is a positive integer; `model` is a nonempty single Codex model name without whitespace. The launcher creates this binding once and does not replace, delete or edit it during a run. On first validation the board creates `pipeline-trial.json` beside it as a durable copy of the complete binding, and rejects a missing or changed binding on subsequent reads. Both files and the controller database must be protected as run state; deleting all run state is outside the board’s trust boundary. `PIPELINE_TRIAL_REQUIRED=1` is propagated to background entries so a missing binding also fails closed there. The controller independently persists the same immutable tuple at run creation. The board reads the binding on each authorization; the controller compares every field with its authoritative record. A missing, malformed, changed or unsupported binding fails closed. Existing non-trial bounded boards continue using their existing protocol; a trial launcher must never remove the trial marker or reuse the isolated board for an unguarded run. The board propagates the binding's paths to background processes through the existing durable binding mechanism.

## Authorization

Before starting a role or delivering a continuation, the board runs the absolute `control` executable without a shell, with a five-second timeout:

```text
control trial-lease --schema 1 --run RUN --token-ceiling N --model MODEL --task TASK --role ROLE --slot N --action ACTION --scope-file ABSOLUTE_PATH
```

Roles are `planning`, `implementation`, `qa`, and `repair`; actions are `launch` and `continue`. The scope file is JSON with task `title` and `description`. Slot is the review round for a reviewer launch and zero for a worker launch or continuation. A fresh or resumed process gets `launch`; a prompt sent to an existing process gets `continue`. A recovery launch is a `launch` and a repair prompt is a `continue` for role `repair`. A context refresh also uses the existing `refresh --handoff ABSOLUTE_PATH` controller operation before the board requests its new launch lease; a resumed launch with a prompt requests both launch and continuation leases. Before a board-generated repair prompt, the board first obtains the continuation lease; the existing controller operation `repair --task TASK --claim native:TASK:REASON:TURN` must also return exactly `REPAIR` with exit 0; it enforces the run-wide repair allowance and deduplicates the claim. Automatic provider-error retry nudges are refused in bounded runs. The controller checks the tuple, role, run status, complete accounting and remaining capacity, then returns exactly `LEASE <id>` on stdout with exit 0. The id is 1–128 ASCII letters, digits or hyphens. On a process launch the board passes this id via `--pipeline-lease`. A continuation's returned authorization does not replace the already running process's lease: the controller must keep that lease subject to the same run-level request gate and revoke or refuse it when the run is held. A lease is permission to attempt requests, not a reservation of tokens.

The board sends the pinned model explicitly to Codex and refuses an explicitly configured different model, a different program, and fallback. The controller independently rejects every request whose effective model differs from the binding. Every request must reserve the measured input plus a provider-enforced output maximum atomically against the same run-wide token ceiling; cached input counts. Unknown or unsettled usage stays reserved. A controller error, timeout, nonzero exit, invalid response, missing accounting, model or binding mismatch, exhausted capacity, run hold, unsupported role or missing gate is a refusal, never a legacy fallback. Denials may print `DENY <reason>` and must exit nonzero; callers do not parse or trust the reason.

## Integration remains necessary

The board fixture proves its side of the contract only. EOB-18 must bind the external planner and handoff sessions before their first request and implement durable atomic reservation and settlement. Integration must additionally prove that the selected provider and runtime enforce the requested output maximum and that every actual request path uses the gate. Until then the hard ceiling and all-role pin are not proven for a trial.
