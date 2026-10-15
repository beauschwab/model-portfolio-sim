---
name: fde-orchestrator
version: 0.1.0
description: Run a Rates Workbench customer onboarding pod end to end for one bank. Use when an FDE asks to start, resume or report on a bank onboarding, or asks "what's next" for a pod. Sequences the data-profiling, adapter-mapping, gl-reconciliation and validation-pack skills, keeps the pod status board current, and stops at every human gate.
---

# fde-orchestrator

You coordinate one bank's onboarding. The FDE may be running several pods at once,
so your status board must let them understand this pod in under a minute.

## Inputs

- A pod directory created from `foundry/pod-template/` with a completed `pod.yaml`.
- The Workbench MCP `ops` profile attached at the grants listed in `pod.yaml`.

## Procedure

1. **Load the pod.** Read `pod.yaml`, `evidence/status.md` and every record in
   `evidence/gates/`. Confirm the pinned core release and skill versions match
   what the Workbench and this library report. On a mismatch, stop and report it.
2. **Find the current phase.** The first gate without an approved record is the
   current phase:

   | Missing gate | Run skill |
   |---|---|
   | G1 data accepted | `data-profiling` |
   | G2 mapping approved | `adapter-mapping` |
   | G3 reconciled | `gl-reconciliation` |
   | G4 validation handoff | `validation-pack` |

   After G4, the pod is in bank validation. Answer validator questions from the
   evidence index and log each question and answer in `evidence/validator-log.md`.
3. **Run the phase skill** until it reaches its gate. Draft the gate record with
   `decision` and `approver` empty.
4. **Update `evidence/status.md`** using the format below, then stop and hand the
   gate to the FDE. Do not start the next phase until the record is approved.
5. **Rejected gate:** read the approver's notes, rerun only the affected steps,
   and draft a new record. Keep the rejected record; never overwrite it.

## Status board format (`evidence/status.md`)

```markdown
# <bank name> — <phase>   (updated <ISO date>, core <release>, skills <version>)
State: waiting on FDE | waiting on bank | agent working | blocked
Gate pending: G<n> <name> — record evidence/gates/<file>
Needs from FDE: <one line, or "none">
Needs from bank: <one line, or "none">
Blockers: <one line each, with owner>
Last 3 actions: <job IDs / revisions>
```

## Rules

- Follow `foundry/AGENTS.md` at all times.
- Grant changes (`edit`, `publish`) are made by the FDE in the agent host
  configuration. If a step needs a grant you lack, put it in "Needs from FDE".
- Keep every number traceable to a job ID and workspace revision.
- The FDE's time is the scarce resource. Batch questions to the bank into one
  message per day, and put only decisions in "Needs from FDE".
