//! The content-addressed store: bytes by hash, write-once. Everything
//! fetched lands here before anything is materialized, so `verify` has one
//! place to re-hash and `gc` one place to sweep.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use dollup_format::{hash_bytes, Hash};

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn open(dir: &Path) -> Result<Store> {
        fs::create_dir_all(dir)?;
        Ok(Store {
            dir: dir.to_path_buf(),
        })
    }

    fn path_for(&self, hash: &Hash) -> Result<PathBuf> {
        let Some((algo, hex)) = hash.0.split_once(':') else {
            bail!("'{hash}' is not an encoded hash");
        };
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) || hex.is_empty() {
            bail!("'{hash}' is not an encoded hash");
        }
        Ok(self.dir.join(algo).join(hex))
    }

    /// Insert bytes; returns their hash. Write is atomic (temp + rename) and
    /// idempotent — the store never holds a truncated blob.
    pub fn put(&self, bytes: &[u8]) -> Result<Hash> {
        let hash = hash_bytes(bytes);
        let path = self.path_for(&hash)?;
        if path.exists() {
            return Ok(hash);
        }
        fs::create_dir_all(path.parent().unwrap())?;
        let tmp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
        fs::write(tmp.path(), bytes)?;
        tmp.persist(&path)
            .with_context(|| format!("persisting {}", path.display()))?;
        Ok(hash)
    }

    /// Fetch bytes, verifying on the way out: a store that returns bytes
    /// not matching their name has been tampered with or corrupted, and
    /// says so rather than passing them along.
    pub fn get(&self, hash: &Hash) -> Result<Option<Vec<u8>>> {
        let path = self.path_for(hash)?;
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        if &hash_bytes(&bytes) != hash {
            bail!(
                "store corruption: {} does not hash to its name",
                path.display()
            );
        }
        Ok(Some(bytes))
    }

    #[cfg_attr(not(test), allow(dead_code))] // the snapshot verbs' first caller
    pub fn contains(&self, hash: &Hash) -> Result<bool> {
        Ok(self.path_for(hash)?.exists())
    }

    /// Every hash present.
    pub fn list(&self) -> Result<Vec<Hash>> {
        let mut out = vec![];
        for algo in read_dir_names(&self.dir)? {
            for hex in read_dir_names(&self.dir.join(&algo))? {
                out.push(Hash(format!("{algo}:{hex}")));
            }
        }
        Ok(out)
    }

    /// Drop everything not in `keep`. Returns how many blobs went.
    pub fn gc(&self, keep: &std::collections::BTreeSet<Hash>) -> Result<usize> {
        let mut swept = 0;
        for hash in self.list()? {
            if !keep.contains(&hash) {
                fs::remove_file(self.path_for(&hash)?)?;
                swept += 1;
            }
        }
        Ok(swept)
    }
}

fn read_dir_names(dir: &Path) -> Result<Vec<String>> {
    let mut names = vec![];
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(names),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        names.push(entry?.file_name().to_string_lossy().into_owned());
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_gc() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let a = store.put(b"alpha").unwrap();
        let b = store.put(b"beta").unwrap();
        assert_eq!(store.get(&a).unwrap().unwrap(), b"alpha");
        assert_eq!(store.put(b"alpha").unwrap(), a, "idempotent");
        let keep = [a.clone()].into_iter().collect();
        assert_eq!(store.gc(&keep).unwrap(), 1);
        assert!(store.get(&b).unwrap().is_none());
        assert!(store.contains(&a).unwrap());
    }
}
