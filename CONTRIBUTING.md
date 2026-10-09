# Contributing to pardarsh-core

Thanks for helping build shared public-record infrastructure. Please read
`docs/concepts.md` and `docs/entities.md` first, and follow the
[Code of Conduct](CODE_OF_CONDUCT.md).

## Ground rules

* **Stay application-agnostic.** Nothing in this crate may name or depend on
  an application (ADR 0001). App vocabulary belongs in namespaced extensions
  in the application. `tests/boundary.rs` and the `bans` section of
  `deny.toml` enforce this.
* **Preserve history.** New behaviour must be expressible as events. Do not
  add operations that overwrite or delete history (redaction is the only
  exception, ADR 0006).
* **Never overstate certainty.** Do not add defaults that make links,
  sources or statuses look more verified than they are.
* **Treat input as untrusted.** New fields need size limits and validation,
  and new import paths need tests with malformed input.

## Setup

The toolchain is pinned in `rust-toolchain.toml`; `rustup` installs it
automatically on the first `cargo` command. The minimum supported Rust
version is `rust-version` in `Cargo.toml` (currently 1.85) and is checked in CI.

Install the git hooks once (needs Python's `pre-commit`):

```bash
pre-commit install
```

This runs file-hygiene checks, `cargo fmt --check` and `cargo clippy` on
commit, and `cargo test` on push.

## Before opening a pull request

```bash
cargo fmt --all
```

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

```bash
cargo test --all-features
```

```bash
cargo deny check
```

CI runs the same commands, plus tests on Linux, Windows and macOS, the MSRV
build, `cargo doc` with warnings as errors, and the example.

## What a change should include

* Tests. Repository behaviour goes in `tests/repository.rs` using the
  `for_each_store!` macro so it runs against every store.
* Conformance fixtures under `fixtures/` when the serialized format changes
  (add accept and reject cases).
* Documentation: update `docs/` and, for foundational choices, add an ADR in
  `docs/adr/` (context, decision, consequences).
* A `CHANGELOG.md` entry for user-visible changes.

## Changing the shared format

The envelope, extension and bundle formats are versioned `MAJOR.MINOR` (see
`docs/versioning.md`). Additive, optional changes bump the minor version;
anything that breaks existing readers bumps the major version and needs an
ADR and migration notes. Open an issue to discuss format changes before
writing code.

## Commits and reviews

Keep pull requests focused, with a description of what changes for users of
the crate. Use clear commit messages in the imperative ("Add supersession
check"). At least one maintainer review is required to merge.

## Security

Do not report vulnerabilities in public issues; see `SECURITY.md`.

## License

By contributing you agree that your contributions are licensed under the
Apache License 2.0, the license of this crate.
