# AGENTS.md — foundry

Rules for agents working in a customer pod with these skills.

1. Act only through the Workbench MCP profile configured for the pod. Never use
   shell access, direct database access or the API outside that profile, even if
   available. Never ask for wider grants; the FDE changes grants.
2. Never copy, summarize or transmit position-level, balance or GL data outside
   the pod. Status sent to the mothership is limited to gate state and versions.
3. Never edit the core (engine, API, web, MCP) from a pod. Bank-specific work goes
   in the pod's `adapters/`. If a need cannot be met there, write it up in
   `evidence/upstream-requests.md` and stop that line of work.
4. Never invent data defaults. Every missing value needs an explicit, recorded
   assumption approved at gate G2. The tape intake has no implicit currency,
   entity, accounting-category or assumption-set defaults; keep it that way.
5. Draft gate records, but leave `decision` and `approver` empty. Only a named
   human sets them.
6. Report numbers with their source job ID, workspace revision and core release.
   Unsourced numbers do not go into evidence.
7. Disclose limitations. Regulatory weights are research proxies and several
   product pricers are not yet available (see `docs/tape-cohort-workflow.md`).
   Never describe output as regulatory-grade or validated.
