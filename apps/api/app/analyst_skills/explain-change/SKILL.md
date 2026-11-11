---
name: explain-change
description: Explain differences between two compatible published Workbench runs with explicit attribution limits.
version: 0.1.0
---

1. Identify the two requested job IDs; ask for them if the selection is ambiguous.
2. Read both statuses, manifests and the relevant bounded tables.
3. Compare source revision, valuation date, scenarios, horizon, units, model identity and policy.
4. If inputs differ outside the requested experiment, report the incompatibility before interpreting changes.
5. Cite native difference/attribution rows when available. Input correlation alone is not causality.
6. Return supported changes, evidence, missing attribution and a proposed controlled experiment.

Never subtract or reconstruct financial outputs in the model. If no native differences
exist, quote the two observations separately and state that attribution is unavailable.
