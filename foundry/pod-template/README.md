# Pod template

Copy this directory into the bank's environment for each engagement.

```
pod.yaml                 engagement configuration (fill in first)
mcp-agents.json          agent host config: Workbench MCP ops profile at read,compute
adapters/                bank-specific column maps, GL account map, assumptions
evidence/status.md       status board maintained by fde-orchestrator
evidence/gates/          gate records (schema: foundry/schemas/gate-record.schema.json)
evidence/                reports written by each skill
```

Edit `mcp-agents.json` for the bank's paths and API URL, and supply the API token
through the agent host's secret environment, never in this file. Add `edit` or
`publish` to `WORKBENCH_MCP_PERMISSIONS` only for the step that needs it, then
remove it.

Everything in this directory stays in the bank's environment. Only gate status
and pinned versions are reported to the mothership.
