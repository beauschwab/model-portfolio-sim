# AGENTS.md — skills

`portfolio-risk-engine/` is the OPERATOR skill: it teaches an LLM agent to USE
the engine (workflows, schemas, performance expectations, disclosure
duties). It is intentionally separate from the AGENTS.md hierarchy, which
documents MODIFYING the engine. Keep them consistent: any behavior change
updates both the skill references and src/portfolio_risk/AGENTS.md. The
canonical skill source lives at the repo author's skill folder; this copy
ships with the repo for SKILLFORGE-style distribution — regenerate the
.skill artifact after edits (skill-creator package_skill).
