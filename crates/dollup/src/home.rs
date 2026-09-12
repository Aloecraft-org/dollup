//! `~/.dollup/`: dollup's own directory, separate from any root.
//!
//! This narrows the "nothing read from home" rule rather than breaking it.
//! Config resolution still reads nothing from here, and no root ever depends
//! on it: a root is self-contained — tar it and move it and it runs — and a
//! link from a root into this directory would make it not so. What lives
//! here is keys, the cache of pulled packages and drt versions, and the list
//! of roots on this box (`roots.json`, written by every verb that opens a
//! root). The cache is read by `audit` and filled by `dollup pull`.

use std::path::PathBuf;

/// `$HOME/.dollup`, or nothing when there is no home to speak of.
pub fn dollup_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(|h| PathBuf::from(h).join(".dollup"))
}

/// A pulled drt release: `~/.dollup/cache/drt/<version>/`, the mirror's own
/// layout — the asset, SHA256SUMS.txt, BUILDINFO.txt — keyed by the version
/// the binary reports, which is what a pin says. `dollup pull drt` writes
/// it; `deploy drt` copies from it; `audit` reads the sums, so a pinned
/// root can be checked offline once its runtime has been pulled once.
pub fn drt_cache_dir(version: &str) -> Option<PathBuf> {
    drt_cache_root().map(|d| d.join(version))
}

/// Every cached release: `~/.dollup/cache/drt/`, one directory per version.
pub fn drt_cache_root() -> Option<PathBuf> {
    dollup_home().map(|h| h.join("cache").join("drt"))
}

/// The sums beside a cached release.
pub fn drt_sums_path(version: &str) -> Option<PathBuf> {
    drt_cache_dir(version).map(|d| d.join("SHA256SUMS.txt"))
}

/// The list of roots on this box, kept by [`crate::roots`].
pub fn roots_path() -> Option<PathBuf> {
    dollup_home().map(|h| h.join("roots.json"))
}

/// The content-addressed store every root on this box shares:
/// `~/.dollup/cache/store`. `pull` fills it and materializes from it;
/// `verify` checks against it; `gc` sweeps it against every recorded
/// root's lock. Content-addressed, so two roots pulling one blob hold one
/// copy, and no root can hand another a different file under the same
/// name — the name is the hash.
pub fn cache_store() -> Option<PathBuf> {
    dollup_home().map(|h| h.join("cache").join("store"))
}
