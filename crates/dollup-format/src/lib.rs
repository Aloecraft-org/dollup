//! The dollup formats (SPEC.md, doc/RepoFormat.md). Pure types plus the two
//! operations that define them: identity hashing and index signing. No IO
//! beyond bytes-in/bytes-out, so a mirror generator, a CI check, or DRT
//! itself can consume this crate without taking the binary's store or fetch
//! machinery.
//!
//! Two deliberate absences. There is no scope anywhere in a manifest —
//! scopes stay host-side, the operator supplies them. And there is no
//! executable anything: every type here deserializes from JSON and does
//! nothing.

pub mod identity;
pub mod index;
pub mod lock;
pub mod manifest;
pub mod sign;
pub mod source;

pub use identity::{hash_bytes, Hash};
pub use index::RepoIndex;
pub use lock::Lockfile;
pub use manifest::Manifest;
pub use source::{Ref, SourceEntry};
