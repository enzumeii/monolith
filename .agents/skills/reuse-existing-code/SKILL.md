---
name: reuse-existing-code
description: Use when implementing, refactoring, or reviewing code that adds helpers, utilities, clients, validators, orchestration loops, test fixtures, or development commands.
---

# Reuse Existing Code

Before adding an implementation, search the owning module and existing common/utils/helpers, equivalent callers, tests, and recipes with `rg`. Read candidate implementations and consumers; matching names alone do not establish compatibility. If source is unavailable, state what remains unverified.

Choose in this order:

| Fit | Action |
| --- | --- |
| Existing contract fits | Reuse directly |
| A concrete gap has a small compatible extension | Enhance the existing implementation and migrate its callers |
| No candidate fits, or reuse crosses domain/security/ownership boundaries or complicates the API | Add focused new code and explain that specific incompatibility |

Record one concise decision in the existing plan or implementation notes: **candidate path/symbol → fit or gap → action → regressions to preserve**. Do not create another report or approval gate.

Different messages, error types, or nearby input shapes require assessing an enhancement before copying. Preserve caller behavior, source diagnostics, lifecycle, and module boundaries; derive statuses from inspected contracts, never helper names. Do not introduce wrappers that only rename a helper or generalize hypothetical future requirements.

For example, extend an existing paginator with caller-controlled error mapping when a new consumer needs resource-specific errors; keep its cursor rules and verify both consumers.

Before completion, inspect the diff for duplicated loops, factories, validators, harnesses, and recipes. Remove superseded code and test existing consumers as well as the new case. Time pressure or a completed duplicate draft does not replace this check. Explicit user requirements take precedence.
