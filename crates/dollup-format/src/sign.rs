//! Index signing (RepoFormat.md §8). The trust anchor is the source entry,
//! never the repo: keys are pinned in the deployment's source list, the
//! signature rides in the tree as `index.json.sig`, and both are spelled in
//! one encoding — `ed25519:<base64>` — so there is nothing to frame or
//! parse.
//!
//! What a good signature means: a holder of a key you pinned signed exactly
//! these index bytes. Not freshness, not revocation; the threat notes carry
//! the limits.

use base64::Engine;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

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

fn decode(spelled: &str, what: &str) -> Result<Vec<u8>, SignError> {
    let b64 = spelled
        .strip_prefix(PREFIX)
        .ok_or_else(|| SignError::KeyFormat(what.to_string()))?;
    B64.decode(b64).map_err(|e| SignError::Bytes(e.to_string()))
}

/// Generate a keypair, spelled: (private, public).
pub fn keygen() -> (String, String) {
    let key = SigningKey::generate(&mut rand_core::OsRng);
    (
        format!("{PREFIX}{}", B64.encode(key.to_bytes())),
        format!("{PREFIX}{}", B64.encode(key.verifying_key().to_bytes())),
    )
}

/// Sign index bytes with a spelled private key; returns the spelled
/// signature — the entire content of `index.json.sig`.
pub fn sign(private_key: &str, index_bytes: &[u8]) -> Result<String, SignError> {
    let bytes = decode(private_key, private_key)?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| SignError::Bytes("private key must be 32 bytes".into()))?;
    let key = SigningKey::from_bytes(&bytes);
    Ok(format!(
        "{PREFIX}{}",
        B64.encode(key.sign(index_bytes).to_bytes())
    ))
}

/// Verify a spelled signature over index bytes against pinned keys; any-of
/// passes (multiple keys exist for rotation, not ceremony). Returns the key
/// that verified, for naming in output.
pub fn verify<'k>(
    keys: &'k [String],
    signature: &str,
    index_bytes: &[u8],
) -> Result<&'k str, SignError> {
    let sig_bytes = decode(signature.trim(), signature).map_err(|_| SignError::SigFormat)?;
    let sig_bytes: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| SignError::Bytes("signature must be 64 bytes".into()))?;
    let sig = Signature::from_bytes(&sig_bytes);
    for spelled in keys {
        let key_bytes: [u8; 32] = decode(spelled, spelled)?
            .try_into()
            .map_err(|_| SignError::Bytes("public key must be 32 bytes".into()))?;
        let key =
            VerifyingKey::from_bytes(&key_bytes).map_err(|e| SignError::Bytes(e.to_string()))?;
        if key.verify(index_bytes, &sig).is_ok() {
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
}
