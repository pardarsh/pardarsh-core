# ADR 0002: Rust, serde/JSON, SQLite

Status: accepted (2026-10-09)

## Context
The workspace was empty; the owner asked for Rust. The brief recommends a
reusable library plus a conventional deployable app, one relational database,
and no graph database or microservices.

## Decision
* Rust 2024 edition, library crate. Typed Rust structs are the single source
  of truth for the format; JSON via serde is the serialization.
* Reference stores: in-memory (tests, tools) and SQLite via `rusqlite`
  (bundled, behind the default `sqlite` feature).
* Dependencies kept small: serde, serde_json, chrono, uuid, sha2, hex,
  thiserror.

## Consequences
JSON Schema documents are not yet published; generating them from the Rust
types (e.g. with `schemars`) is a follow-up so non-Rust consumers get a
machine-readable contract. Conformance fixtures already provide
language-neutral accept/reject cases.
