//! Index signing (RepoFormat.md §8): the *spelling* of keys and signatures,
//! `ed25519:<base64>` for both so there is nothing to frame or parse, over
//! the one signing implementation — drt-config's. The trust anchor is the
//! source entry, never the repo: keys are pinned in the deployment's source
//! list, and the signature rides in the tree as `index.json.sig`.
//!
//! The cryptography lives in `drt_config::sign` and nowhere else. drt
//! verifies consent approvals with it at start; dollup signs and verifies
//! indexes with it here; and one implementation is what keeps the two able
//! to verify each other's output. This file adds the prefix, the key-file
//! shape, and the any-of-these-keys rule, and nothing cryptographic.
//!
//! What a good signature means: a holder of a key you pinned signed exactly
//! these index bytes. Not freshness, not revocation; the threat notes carry
//! the limits.

use base64::Engine;
use drt_config::sign::{PublicKey, SecretKey, Signature};

const B64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;
const PREFIX: &str = "ed25519:";

#[derive(Debug, thiserror::Error)]
pub enum SignError {
    #[error("'{0}' is not an ed25519 key: expected `ed25519:<base64>`")]
    KeyFormat(String),
    #[error("signature is not `ed25519:<base64>`")]
    SigFormat,
    #[error("bad key or signature bytes: {0}")]
    Bytes(String),
    #[error("signature does not verify under any pinned key")]
    Verify,
}

/// The base64 after the prefix, or the format refusal naming `what`.
fn unprefixed<'s>(spelled: &'s str, what: &str) -> Result<&'s str, SignError> {
    spelled
        .strip_prefix(PREFIX)
        .ok_or_else(|| SignError::KeyFormat(what.to_string()))
}

fn spell(bytes: &[u8]) -> String {
    format!("{PREFIX}{}", B64.encode(bytes))
}

/// A spelled private key to the signing key it seeds. The file holds the
/// 32-byte seed, which is what `SecretKey::seed_bytes` writes back, so a
/// key generated before the signer moved upstream still reads.
fn secret(private_key: &str) -> Result<SecretKey, SignError> {
    let bytes = B64
        .decode(unprefixed(private_key, private_key)?)
        .map_err(|e| SignError::Bytes(e.to_string()))?;
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|_| SignError::Bytes("private key must be 32 bytes".into()))?;
    Ok(SecretKey::from_seed(&seed))
}

/// Generate a keypair, spelled: (private, public).
///
/// Entropy is this crate's to supply — the signer takes 32 bytes and
/// deliberately chooses no RNG on a caller's behalf — and it comes from the
/// OS and nowhere else.
pub fn keygen() -> (String, String) {
    let mut random = [0u8; 32];
    getrandom::getrandom(&mut random).expect("the OS supplies entropy");
    let key = SecretKey::generate(random);
    (spell(&key.seed_bytes()), spell(key.public_key().as_bytes()))
}

/// The public key that belongs to a spelled private key.
///
/// Publishing scripts have been hunting for a `.pub` file *beside* the
/// private key — `${key%.*}.pub`, then `$key.pub`, then an error naming
/// both — which is guesswork about a filename standing in for a fact the
/// key itself carries. Worse, the two can drift: sign with one key, pin the
/// public half of another, and the repo you just signed is one you cannot
/// add. Derived, they cannot.
pub fn public_key_of(private_key: &str) -> Result<String, SignError> {
    Ok(spell(secret(private_key)?.public_key().as_bytes()))
}

/// Sign index bytes with a spelled private key; returns the spelled
/// signature — the entire content of `index.json.sig`.
pub fn sign(private_key: &str, index_bytes: &[u8]) -> Result<String, SignError> {
    let signature = secret(private_key)?.sign(index_bytes);
    Ok(format!("{PREFIX}{}", String::from(signature)))
}

/// Verify a spelled signature over index bytes against pinned keys; any-of
/// passes (multiple keys exist for rotation, not ceremony). Returns the key
/// that verified, for naming in output.
pub fn verify<'k>(
    keys: &'k [String],
    signature: &str,
    index_bytes: &[u8],
) -> Result<&'k str, SignError> {
    let signature = unprefixed(signature.trim(), signature).map_err(|_| SignError::SigFormat)?;
    let signature = Signature::try_from(signature.to_string()).map_err(SignError::Bytes)?;
    for spelled in keys {
        let key = PublicKey::try_from(unprefixed(spelled, spelled)?.to_string())
            .map_err(SignError::Bytes)?;
        if key.verify(index_bytes, &signature).is_ok() {
            return Ok(spelled);
        }
    }
    Err(SignError::Verify)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify_round_trip() {
        let (private, public) = keygen();
        let sig = sign(&private, b"index bytes").unwrap();
        assert_eq!(
            verify(std::slice::from_ref(&public), &sig, b"index bytes").unwrap(),
            public
        );
        assert!(matches!(
            verify(std::slice::from_ref(&public), &sig, b"tampered"),
            Err(SignError::Verify)
        ));
        let (_, other) = keygen();
        assert!(
            verify(&[other, public], &sig, b"index bytes").is_ok(),
            "any-of"
        );
    }

    #[test]
    fn the_spellings_are_refused_by_name() {
        let (private, public) = keygen();
        let sig = sign(&private, b"x").unwrap();
        assert!(matches!(
            verify(&["RZNTaXSe".to_string()], &sig, b"x"),
            Err(SignError::KeyFormat(_))
        ));
        assert!(matches!(
            verify(std::slice::from_ref(&public), "not-a-signature", b"x"),
            Err(SignError::SigFormat)
        ));
        // Right prefix, wrong length: the signer names the byte count.
        let err = verify(&[format!("{PREFIX}AAAA")], &sig, b"x").unwrap_err();
        assert!(matches!(err, SignError::Bytes(_)), "{err}");
        assert!(err.to_string().contains("ed25519 wants 32"), "{err}");
    }
}

#[cfg(test)]
mod derive_tests {
    use super::*;

    #[test]
    fn the_derived_public_key_is_the_one_keygen_produced() {
        let (private, public) = keygen();
        assert_eq!(public_key_of(&private).unwrap(), public);
    }

    #[test]
    fn a_derived_key_verifies_what_its_private_half_signed() {
        let (private, _) = keygen();
        let derived = public_key_of(&private).unwrap();
        let sig = sign(&private, b"index bytes").unwrap();
        assert!(verify(std::slice::from_ref(&derived), &sig, b"index bytes").is_ok());
    }
}
