//! Sources and refs (SPEC.md §3, RepoFormat.md §2). A source is a URL plus,
//! optionally, pinned keys; four schemes, one format, interchangeable
//! because identity is content.

use serde::{Deserialize, Serialize};

/// One line of the deployment's source list. A bare string is a valid
/// source and means *unsigned*.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SourceEntry {
    Url(String),
    Signed {
        url: String,
        #[serde(default)]
        keys: Vec<String>,
    },
}

impl SourceEntry {
    pub fn url(&self) -> &str {
        match self {
            SourceEntry::Url(u) => u,
            SourceEntry::Signed { url, .. } => url,
        }
    }

    pub fn keys(&self) -> &[String] {
        match self {
            SourceEntry::Url(_) => &[],
            SourceEntry::Signed { keys, .. } => keys,
        }
    }

    pub fn scheme(&self) -> Result<Scheme, RefError> {
        Scheme::of(self.url())
    }
}

/// Where bytes come from once a wrapper is stripped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Https,
    File,
}

/// The v1 schemes: a transport, optionally wrapped. `zip+` and `git+` wrap
/// either transport (a zipball on disk and a git file remote are both
/// legitimate), and growth is additive; an unknown scheme fails by name,
/// never silently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// A static repo: index, then blobs or tree paths.
    Plain(Transport),
    /// One archive, fetched whole and extracted. Any forge's zipball URL is
    /// such an archive; there is deliberately no forge adapter.
    Zip(Transport),
    /// A git remote; tags and branches are input, commits are recorded.
    Git(Transport),
}

impl Scheme {
    pub fn of(url: &str) -> Result<Scheme, RefError> {
        let (wrap, rest) = if let Some(r) = url.strip_prefix("zip+") {
            (Some(Scheme::Zip as fn(Transport) -> Scheme), r)
        } else if let Some(r) = url.strip_prefix("git+") {
            (Some(Scheme::Git as fn(Transport) -> Scheme), r)
        } else {
            (None, url)
        };
        let transport = if rest.starts_with("https://") {
            Transport::Https
        } else if rest.starts_with("file://") {
            Transport::File
        } else {
            return Err(RefError::UnknownScheme(url.to_string()));
        };
        Ok(match wrap {
            Some(w) => w(transport),
            None => Scheme::Plain(transport),
        })
    }

    pub fn transport(&self) -> Transport {
        match self {
            Scheme::Plain(t) | Scheme::Zip(t) | Scheme::Git(t) => *t,
        }
    }

    /// Does an unsigned source of this scheme trip `require_signatures`?
    /// File transports are exempt: a local path's trust story is the
    /// filesystem's, however it is wrapped.
    pub fn network(&self) -> bool {
        self.transport() == Transport::Https
    }
}

/// What `dollup add` takes: a package name with an optional version
/// requirement, optionally pinned to one source —
/// `name`, `name@^1.2`, `<source-url>#name@^1.2`. A bare name resolves
/// against the deployment's source list, in order.
#[derive(Debug, Clone, PartialEq)]
pub struct Ref {
    pub source: Option<String>,
    pub name: String,
    pub version: Option<semver::VersionReq>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RefError {
    #[error("'{0}' names no scheme dollup knows (https, zip+https, git+https, file)")]
    UnknownScheme(String),
    #[error("'{0}' is not a version requirement: {1}")]
    BadVersionReq(String, String),
    #[error("a ref needs a package name: '{0}'")]
    Empty(String),
}

impl std::str::FromStr for Ref {
    type Err = RefError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (source, rest) = match s.rsplit_once('#') {
            Some((src, rest)) => {
                Scheme::of(src)?;
                (Some(src.to_string()), rest)
            }
            None => (None, s),
        };
        let (name, version) = match rest.split_once('@') {
            Some((n, v)) => (
                n,
                Some(
                    v.parse::<semver::VersionReq>()
                        .map_err(|e| RefError::BadVersionReq(v.to_string(), e.to_string()))?,
                ),
            ),
            None => (rest, None),
        };
        if name.is_empty() {
            return Err(RefError::Empty(s.to_string()));
        }
        Ok(Ref {
            source,
            name: name.to_string(),
            version,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs_parse() {
        let r: Ref = "hello".parse().unwrap();
        assert_eq!((r.source, r.name, r.version), (None, "hello".into(), None));

        let r: Ref = "hello@^1.2".parse().unwrap();
        assert!(r.version.unwrap().matches(&"1.3.0".parse().unwrap()));

        let r: Ref = "zip+https://github.com/o/r/archive/refs/heads/main.zip#can@^0.1"
            .parse()
            .unwrap();
        assert_eq!(r.name, "can");
        assert_eq!(
            Scheme::of(r.source.as_deref().unwrap()).unwrap(),
            Scheme::Zip(Transport::Https)
        );
        assert_eq!(
            Scheme::of("git+file:///srv/repo.git").unwrap(),
            Scheme::Git(Transport::File)
        );
        assert!(!Scheme::of("git+file:///srv/repo.git").unwrap().network());

        assert!(matches!(
            "ftp://x#y".parse::<Ref>(),
            Err(RefError::UnknownScheme(_))
        ));
    }

    #[test]
    fn source_entries_take_both_spellings() {
        let list: Vec<SourceEntry> = serde_json::from_str(
            r#"["file:///tmp/repo",
                {"url": "https://dollup.aloecraft.org/std-repo/", "keys": ["ed25519:AAAA"]}]"#,
        )
        .unwrap();
        assert!(list[0].keys().is_empty());
        assert_eq!(list[1].keys().len(), 1);
        assert!(list[1].scheme().unwrap().network());
        assert!(!list[0].scheme().unwrap().network());
    }
}
