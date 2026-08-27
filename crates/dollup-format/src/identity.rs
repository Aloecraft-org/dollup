//! Identity is the content hash (SPEC.md §3). One algorithm per store,
//! named in every encoded hash so a future migration is a re-hash, not a
//! format break.
//!
//! Two identities per package, and the split is load-bearing (RepoFormat.md
//! §4a): **package identity** covers everything the package contains and is
//! what the lockfile records; **code-set identity** covers the guest face
//! alone and is what an instance pins at spawn — the guest face is the only
//! part ever registered into an instance, so a host-face fix must not move
//! the pin a sleeping agent restores against.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// An encoded hash: `sha256:<hex>`. Serialized as the string.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Hash(pub String);

impl Hash {
    pub fn algorithm(&self) -> Option<&str> {
        self.0.split_once(':').map(|(a, _)| a)
    }
}

impl fmt::Display for Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Hash one blob.
pub fn hash_bytes(bytes: &[u8]) -> Hash {
    Hash(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

/// The canonical form for a set of files: `<hash>  <path>\n` lines sorted by
/// path (byte order), hashed as one document. The same shape `sha256sum`
/// prints, so a shell can reproduce it.
pub fn hash_file_set(files: &BTreeMap<String, Hash>) -> Hash {
    let mut doc = String::new();
    for (path, hash) in files {
        doc.push_str(&hash.0);
        doc.push_str("  ");
        doc.push_str(path);
        doc.push('\n');
    }
    hash_bytes(doc.as_bytes())
}

/// Package identity: the file set plus the manifest itself, keyed under the
/// name the tree stores it as. Hashing the manifest bytes pins name,
/// version, faces, and requirements transitively.
pub fn package_identity(manifest_bytes: &[u8], files: &BTreeMap<String, Hash>) -> Hash {
    let mut all = files.clone();
    all.insert("manifest.json".into(), hash_bytes(manifest_bytes));
    hash_file_set(&all)
}

/// Code-set identity: the guest face alone — entry module name plus the
/// guest files. The `main` line is part of the identity because renaming the
/// entry changes what runs without changing any file.
pub fn code_set_identity(main: &str, guest_files: &BTreeMap<String, Hash>) -> Hash {
    let mut doc = format!("main {main}\n");
    for (path, hash) in guest_files {
        doc.push_str(&hash.0);
        doc.push_str("  ");
        doc.push_str(path);
        doc.push('\n');
    }
    hash_bytes(doc.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(pairs: &[(&str, &str)]) -> BTreeMap<String, Hash> {
        pairs
            .iter()
            .map(|(p, b)| (p.to_string(), hash_bytes(b.as_bytes())))
            .collect()
    }

    #[test]
    fn file_set_hash_is_order_independent_and_content_sensitive() {
        let a = set(&[("b.dlua", "two"), ("a.dlua", "one")]);
        let b = set(&[("a.dlua", "one"), ("b.dlua", "two")]);
        assert_eq!(hash_file_set(&a), hash_file_set(&b));
        let c = set(&[("a.dlua", "one"), ("b.dlua", "changed")]);
        assert_ne!(hash_file_set(&a), hash_file_set(&c));
    }

    #[test]
    fn host_face_change_moves_package_identity_but_not_code_set() {
        let guest = set(&[("guest/can.dlua", "return 1")]);
        let mut all = guest.clone();
        all.insert("host/can.wasm".into(), hash_bytes(b"v1"));
        let manifest = br#"{"name":"can"}"#;
        let pkg1 = package_identity(manifest, &all);
        let code1 = code_set_identity("can", &guest);

        all.insert("host/can.wasm".into(), hash_bytes(b"v2 - connector fix"));
        let pkg2 = package_identity(manifest, &all);
        let code2 = code_set_identity("can", &guest);

        assert_ne!(pkg1, pkg2, "the lockfile sees the fix");
        assert_eq!(code1, code2, "the sleeping agent's pin does not move");
    }

    #[test]
    fn renaming_the_entry_module_moves_the_code_set() {
        let guest = set(&[("guest/a.dlua", "x"), ("guest/b.dlua", "y")]);
        assert_ne!(code_set_identity("a", &guest), code_set_identity("b", &guest));
    }
}
