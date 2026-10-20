---
name: monolith-protocol-enums
description: Use when related provider protocol codes or magic strings control Rust API behavior and meaningful enum variants can express their semantics.
---

# Protocol enums

Group related wire values into a provider-owned enum when callers branch on their meaning. Keep these types beside the provider models in `services/api/src/models/`; shared domain DTOs remain provider-independent.

Before replacing strings, inspect decoding, serialization, comparisons, filtering, deduplication and public error behavior. Record the existing unknown-value policy and preserve it.

| Existing contract | Enum representation |
| --- | --- |
| Unknown codes decode and their identities matter | `Other(String)` with exact raw string round trips |
| Unknown codes decode but are ignored | Explicit unknown handling that retains the skip behavior |
| Unknown codes already fail decoding | Preserve rejection unless the task changes the contract |
| Arbitrary customer text or identifiers | Keep the string |

Give known variants semantic names and map exact wire spellings through Serde conversions or explicit names. For example, a provider payment code can become `Payment`, while `Other(raw)` retains an unrecognized code used in a dedup key. A unit `#[serde(other)]` variant loses raw identity and cannot preserve that contract.

Match enums in service policy rather than converting them back to strings for branching. Serialize or expose `as_str()` at wire, description and identity boundaries. Unknown currency must retain the existing failure behavior; choosing a default precision or exchange rate changes amounts.

Cover known values, unknown round trips, distinct unknown identities, and existing failure/skip behavior in model and service tests. Keep free text and unrelated string groups outside the refactor.
