## Summary

<!-- What does this change do for users of the crate? -->

## Motivation

<!-- Why is this change needed? Link the issue if there is one. -->

## Changes

<!-- Briefly describe the implementation. -->

## Testing

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo test --all-features`
- [ ] `cargo deny check`
- [ ] Repository behaviour is tested with `for_each_store!` (memory and SQLite)

## Format / schema impact

<!-- Changes to the serialized envelope, extensions, events or bundle format (docs/versioning.md). -->

- [ ] No change to the shared format
- [ ] Additive change: minor version bumped, conformance fixtures added
- [ ] Breaking change: major version bumped, ADR and migration notes added

## Checklist

- [ ] Nothing application-specific added to the core (docs/adr/0001-repository-boundary.md)
- [ ] History is preserved; no new operation overwrites or deletes events
- [ ] New input is validated and size-limited
- [ ] Docs, ADRs and `CHANGELOG.md` updated where relevant

## Notes

<!-- Anything reviewers should pay particular attention to. -->
