---
name: job-recovery
description: Recover visibility into an interrupted durable job without resubmitting or cancelling financial work.
version: 0.1.0
---

1. Identify the original job ID or exact idempotency key; never invent a replacement.
2. Discover with get_jobs, then inspect the selected job and any decision session.
3. Preserve the original status, input revision, error and source identities.
4. For completed jobs inspect manifest and verification evidence. For running jobs report progress.
5. For failures explain only supported prerequisites, including rebuild/restart or revision conflicts.
6. Return a recovery receipt: original key/job, status, evidence, unresolved error and proposed next action.

This workflow is read-only. It does not retry a submission, resume a solver, cancel a job,
restart services or change saved state. A stopped analyst conversation leaves financial jobs running.
