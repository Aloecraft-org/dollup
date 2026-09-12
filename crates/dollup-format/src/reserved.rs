//! Names a root's layout claims for itself, refused wherever a name is
//! chosen: a package, a profile, a node, a project. `.drt_root/` holds
//! `init/`, `live/`, `log/`, `profile/`, `state/` and the `drt` binary, and a
//! package called `live` is a collision waiting for the first tool that joins
//! names onto that directory.
//!
//! The list is drt-config's — [`RESERVED`] is `drt_config::project::RESERVED`
//! re-exported, so drt and dollup refuse the same six from one constant and
//! cannot drift. What this module adds is the sentence: a refusal that names
//! what was tried, what it collided with, and the whole list, worded once so
//! the manifest check and the ref parser say the same thing.

pub use drt_config::project::RESERVED;

/// The reserved name `name` collides with, if any — the canonical spelling,
/// for naming in a refusal. Case-insensitive, as drt's own check is: `Live`
/// collides with `live/` on a case-folding filesystem, and refusing it
/// everywhere keeps the rule from depending on where a package lands.
pub fn reserved(name: &str) -> Option<&'static str> {
    RESERVED
        .iter()
        .copied()
        .find(|r| r.eq_ignore_ascii_case(name))
}

/// One sentence for every refusal, wherever it is raised.
pub fn refusal(name: &str, reserved: &str) -> String {
    format!(
        "'{name}' is a reserved name: `{reserved}` is part of a root's own \
         layout, so no package, profile, node or project may be called that. \
         Reserved whatever the capitalization: {}",
        RESERVED.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_six_are_refused_in_any_capitalization_and_nothing_else_is() {
        // The six, as this side of the boundary understands them; a change
        // upstream shows up here as a list that no longer reads right.
        assert_eq!(RESERVED, ["drt", "init", "live", "log", "profile", "state"]);
        for r in RESERVED {
            assert_eq!(reserved(r), Some(*r));
            assert_eq!(reserved(&r.to_uppercase()), Some(*r));
            assert!(drt_config::project::is_reserved(r), "drt agrees on {r}");
        }
        assert_eq!(reserved("Live"), Some("live"));
        // Names that merely contain one are fine; the collision is exact.
        assert_eq!(reserved("lives"), None);
        assert_eq!(reserved("init-tools"), None);
        assert_eq!(reserved("hello"), None);
        assert_eq!(reserved(""), None);
    }
}
