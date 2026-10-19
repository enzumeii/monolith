---
name: monolith-rust-file-layout
description: Use when adding, moving, or reviewing Rust items and test-module declarations in this monolith repository.
---

# Rust file layout

Read the root [AGENTS.md](../../../AGENTS.md) and the touched module before placing items.

- Put exported functions before private helpers, including within impl blocks.
- Put test-gated child-module declarations after all production items in their owning source file. Keep each declaration and its attributes together at the bottom.
- Keep unit-test implementations in the existing mirrored `src/tests/` paths; moving a declaration does not justify moving test bodies into production files.
- Preserve private access, serde hooks, feature gates and module paths when rearranging items. Limit reordering to files already in scope.
- Inspect the final touched files for production items after test-module declarations, then run the owning crate's formatter/check and relevant tests.

Example final items:

```rust
fn private_helper() {}

#[cfg(test)]
#[path = "../tests/feature.rs"]
mod tests;
```
