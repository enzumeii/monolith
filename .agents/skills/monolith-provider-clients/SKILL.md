---
name: monolith-provider-clients
description: Use when adding or changing API provider clients or moving business behavior across client and service boundaries in this repository.
---

# Provider client boundaries

- Read the provider method and its consumers before changing ownership. Keep one method per generic external operation; mirror SDK request/response types where practical.
- Clients own provider transport, authentication, timeout, protocol decoding and error-source preservation. Construct shared clients once in AppState.
- Services own business payloads, sender/content policy, report attachments, pagination, aggregation and multi-call workflows. Scheduler loops, leases and retry timing belong to the top-level cron runtime.
- ResendClient exposes one generic send_email accepting SDK options (including optional SDK idempotency wrapping). services/email.rs builds scheduled-report requests; reship owns its notification content.
- Cron business services are services/cron/config.rs, report.rs and job.rs. SQL belongs to the corresponding repositories; shared report tables/CSV belong to report-generator.
- Update existing callers and provider HTTP fixtures together. Confirm generic client changes preserve request wire shape, timeout and sanitized error behavior without duplicating business methods in the client.
