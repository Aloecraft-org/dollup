//! drt-config's canonical-JSON golden vector, asserted from inside dollup's
//! workspace: consent.md acceptance 6, seen from the consumer's side.
//!
//! dollup's workspace enables `serde_json/preserve_order` (deliberately, for
//! the artifact formats); drt's does not; and cargo unifies features per
//! workspace — so the one shared canonicalizer is compiled in two states.
//! drt-config's own CI runs its suite under both. This is the same vector
//! checked where dollup actually builds it, so a disagreement between the
//! two binaries is a failing test in whichever repository bumped second,
//! rather than a signature nobody can verify.
//!
//! Pinned by hand from drt-config's `tests/fixtures/canonical.{json,bytes,
//! sha256}` at rev 375ef49c. When drt changes the format its fixture moves
//! and this copy does not, so the dependency bump carrying the change fails
//! here — which is the moment to look.

use drt_config::canon;

/// The input, keys deliberately out of order. Under `preserve_order` this
/// parses into a map that iterates in exactly this order, which is what a
/// canonicalizer trusting the map would emit.
const INPUT: &str = r#"{"realm":"operator.net.domains","ask":{"add":["example.com","a.example.com"]},"node":"root/intake","root_id":"0192f0c1-8000-7000-8000-00000000abcd","valid_until":"2026-09-19T00:00:00Z"}"#;

const BYTES: &str = r#"{"ask":{"add":["example.com","a.example.com"]},"node":"root/intake","realm":"operator.net.domains","root_id":"0192f0c1-8000-7000-8000-00000000abcd","valid_until":"2026-09-19T00:00:00Z"}"#;

const SHA256: &str = "sha256:3d67f4ceac8e50ea5b27a5a05388b005bb6b961da90dfd57a40897e311c96792";

#[test]
fn the_shared_canonicalizer_emits_drts_bytes_under_preserve_order() {
    let value: serde_json::Value = serde_json::from_str(INPUT).unwrap();
    // The premise, checked rather than assumed: this workspace really does
    // build serde_json with insertion-ordered maps, so what follows tests
    // the state it claims to. If preserve_order is ever dropped here, this
    // line is the one to update, and drt's own suite covers the other state.
    let keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["realm", "ask", "node", "root_id", "valid_until"],
        "preserve_order is on in this workspace"
    );

    let bytes = canon::to_canonical_bytes(&value);
    assert_eq!(String::from_utf8(bytes.clone()).unwrap(), BYTES);
    assert_eq!(canon::hash(&bytes).as_str(), SHA256);
}
