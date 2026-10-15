# Foundry — FDE skill library and customer pods

The foundry lets one forward-deployed engineer (FDE) bring several banks onto the
Rates Workbench in parallel. Agents do the integration work; the FDE approves at
gates and talks to the bank. This directory is operating material for that work.
It contains no financial calculations and changes no engine, API or UI contract.

## Model

```
Mothership (this repository)                 Bank pod (inside each bank's environment)
---------------------------------            ------------------------------------------
canonical core (engine/API/UI)  --release--> pinned core release
foundry/skills (versioned)      --release--> pinned skill versions
pod-template/                   --copy-----> pod.yaml, adapters/, evidence/
                                <-status---- gate records and skill versions only
```

- **Bank data never leaves the bank.** Agents run inside the bank's tenant, against
  the bank's own Workbench API through the `apps/mcp` server. The mothership
  receives gate status and skill/core versions, never positions, balances or GL.
- **The core is never edited in a pod.** Bank-specific work lives in `adapters/`
  (column maps, account maps) and in the engine's documented extension recipes.
  Anything that needs a core change is raised upstream as an issue.
- **Agents act through role-limited MCP profiles.** Pods start with the `ops`
  profile at `read,compute`. `edit` and `publish` grants are added only for the
  step that needs them and only after the preceding gate is approved
  (see `apps/mcp/README.md`).

## Skills

| Skill | Phase | Output | Human gate |
|---|---|---|---|
| [`fde-orchestrator`](skills/fde-orchestrator/SKILL.md) | All | Pod status board, next actions | FDE owns every gate |
| [`data-profiling`](skills/data-profiling/SKILL.md) | Week 1 | Data-quality report, gap list | G1: data accepted |
| [`adapter-mapping`](skills/adapter-mapping/SKILL.md) | Weeks 1–2 | Tape column maps, imported tapes, cohort builds | G2: mapping approved |
| [`gl-reconciliation`](skills/gl-reconciliation/SKILL.md) | Weeks 2–4 | Book-to-GL reconciliation, break explanations | G3: reconciled within tolerance |
| [`validation-pack`](skills/validation-pack/SKILL.md) | Month 1 onward | Model-risk documentation draft and evidence index | G4: handed to bank validators |

Planned next: `parallel-run`, `behavioral-calibration`, `customization`, `upgrade`,
`alco-reporting`. Add each as `skills/<name>/SKILL.md` with the same structure.

## Gates

Every gate produces a record in the pod's `evidence/gates/` that conforms to
[`schemas/gate-record.schema.json`](schemas/gate-record.schema.json). A gate is
passed only when a named human approver is recorded. Agents may draft the record;
they may never set `decision` themselves.

| Gate | Approver | Passing condition |
|---|---|---|
| G1 data accepted | FDE + bank data owner | Required fields present, quality exceptions dispositioned |
| G2 mapping approved | FDE | Every canonical field mapped or explicitly defaulted; imports conserve record counts and balances |
| G3 reconciled | FDE + bank controller | Book balances tie to the trial balance within the pod's tolerance; every break explained |
| G4 validation handoff | FDE | Validation pack complete; limitations and open issue #25 gates disclosed |

## Starting a pod

1. Copy `pod-template/` into the bank's environment and fill in `pod.yaml`.
2. Pin the core release and skill versions in `pod.yaml`.
3. Configure the agent host with `pod-template/mcp-agents.json` (ops profile, `read,compute`).
4. Ask the agent to run `fde-orchestrator` with the pod directory.

## Versioning

Each skill declares `version` in its frontmatter. Bump the minor version for
added steps and the major version for changed gate criteria or outputs. Pods pin
versions; the `upgrade` skill (planned) moves a pod forward and reruns affected
evidence. Record changes in [`CHANGELOG.md`](CHANGELOG.md).
