---
name: monolith-sql-formatting
description: Use when adding, changing, or reviewing SQL queries, migrations, database fixtures, or SQL fault controls in this repository.
---

# SQL formatting

Read the affected query and its callers before editing. Inspect existing formatter configuration and owning `just` recipes: rustfmt formats Rust, and API dprint covers GraphQL/JSON; neither formats embedded SQL. Use readable multiline SQL without adding a formatter dependency solely for a query change.

- Format every SQL statement introduced or touched in the requested change, including migrations, native/E2E fixtures, cleanup queries and fault triggers. Keep unrelated baseline queries outside scope.
- Put major clauses (`SELECT`, `FROM`, `WHERE`, `ORDER BY`, `RETURNING`) on separate lines, uppercase SQL keywords and indent nested queries, column/value lists, assignments and procedural bodies consistently. Use spaces around operators and after commas. Short commands such as `BEGIN` may remain on one line.
- Keep Rust query construction, each bind, execution and await readable on separate lines. Macro/async bodies sometimes retain compressed chains after rustfmt; inspect them directly.
- Preserve bind positions/types, casts, constraints, exact `NUMERIC` expressions, transaction boundaries and row locks. Formatting a timestamp must preserve its type and instant; it does not authorize timezone conversions or migration-history changes.
- Find consumers that observe query text, including `pg_stat_activity` race tests, and update their whitespace matching while preserving what they synchronize.

For example, the convention row lock retains its predicate, bind and lock:

```rust
sqlx::query_as(
    "SELECT *
     FROM conventions
     WHERE id = $1
     FOR UPDATE",
)
.bind(id)
.fetch_optional(connection)
.await?
```

Review the SQL token/bind diff, then run the owning formatter/check and database/E2E recipes required by the change. Formatting does not replace connection-level regression coverage of constraints, rollback, commit faults or concurrent updates.
