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

/// Where a pulled drt version's release sums sit: the mirror's own layout
/// under the cache, keyed by the pinned spelling. `dollup pull drt
/// <version>` will write it; `audit` reads it, so a pinned root can be
/// checked offline once its runtime has been pulled once.
pub fn drt_sums_path(version: &str) -> Option<PathBuf> {
    dollup_home().map(|h| {
        h.join("cache")
            .join("drt")
            .join(version)
            .join("SHA256SUMS.txt")
    })
}

/// The list of roots on this box, kept by [`crate::roots`].
pub fn roots_path() -> Option<PathBuf> {
    dollup_home().map(|h| h.join("roots.json"))
}
