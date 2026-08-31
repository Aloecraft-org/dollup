//! One HTTPS agent, with a root store that works in both places dollup runs.
//!
//! The two environments pull in opposite directions and ureq's features let
//! you have exactly one of them:
//!
//! - **Behind a proxy or a private CA** — a corporate MITM box, a CI egress
//!   proxy, an internal PKI. The roots that matter are in the *system* store
//!   and compiled-in roots know nothing about them. ureq's `native-certs`
//!   feature covers this.
//! - **On a bare box** — a static musl binary on a scratch container, or an
//!   Alpine image without `ca-certificates`. There is no system store at
//!   all, and `native-certs` returns an empty set. ureq's default
//!   `webpki-roots` covers this.
//!
//! ureq picks one at compile time: `root_certs()` is `#[cfg(feature =
//! "native-certs")]` or `#[cfg(not(...))]`, mutually exclusive, no fallback.
//! Choosing `native-certs` broke the bare box — every HTTPS request failing
//! with nothing but a `log::error!` that dollup never initialises a logger to
//! print. Choosing the default broke the proxy, which is how this started.
//!
//! So dollup builds the **union** and hands it to ureq as an agent config.
//! Native certs are added when the system has them and silently skipped when
//! it does not; webpki's roots are always there underneath. Neither
//! environment is the one that breaks.
//!
//! The construction mirrors `ureq::rtls::default_tls_config` deliberately,
//! `builder_with_provider` included — rustls 0.23 panics from `builder()`
//! when no process-wide crypto provider is installed, and ureq's own comment
//! says that is why it does not use it.

use std::sync::{Arc, OnceLock};

/// The shared agent. Built once; ureq agents are cheap to clone and pool
/// connections, which is what makes fetching a repo index and then its
/// packages one connection rather than several.
pub fn agent() -> ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(build).clone()
}

fn build() -> ureq::Agent {
    ureq::AgentBuilder::new().tls_config(Arc::new(client_config())).build()
}

fn client_config() -> rustls::ClientConfig {
    rustls::ClientConfig::builder_with_provider(rustls::crypto::ring::default_provider().into())
        .with_protocol_versions(&[&rustls::version::TLS12, &rustls::version::TLS13])
        .expect("the ring provider supports TLS 1.2 and 1.3")
        .with_root_certificates(roots())
        .with_no_client_auth()
}

/// webpki's roots, plus whatever the system trusts.
fn roots() -> rustls::RootCertStore {
    // Absent, unreadable, or empty is the ordinary case on a minimal image,
    // not an error.
    roots_from(rustls_native_certs::load_native_certs().unwrap_or_default())
}

/// The union, with the system's contribution passed in so the empty case —
/// the whole point of this module — is testable without a machine that has
/// no certificates on it. Order does not matter: a root store is a set, and
/// a chain to either source is accepted.
fn roots_from(
    native: Vec<rustls::pki_types::CertificateDer<'static>>,
) -> rustls::RootCertStore {
    let mut store = rustls::RootCertStore::empty();
    store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let (_added, _ignored) = store.add_parsable_certificates(native);
    store
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_no_system_certs_at_all_webpkis_roots_still_stand() {
        // THE bare-box invariant, and the reason this module exists: a
        // static binary on a scratch container has no system store, and
        // ureq's `native-certs` feature would leave it with zero roots and
        // a log line nobody prints. Here it still has webpki's.
        let bare = roots_from(vec![]);
        assert_eq!(bare.len(), webpki_roots::TLS_SERVER_ROOTS.len());
        assert!(bare.len() > 0, "webpki ships roots");
    }

    #[test]
    fn a_system_store_adds_to_them_rather_than_replacing_them() {
        // And the proxy/private-CA invariant: whatever the machine trusts
        // is added on top, never instead.
        let bare = roots_from(vec![]).len();
        assert!(
            roots().len() >= bare,
            "the union must never be smaller than webpki's roots alone"
        );
    }

    #[test]
    fn a_config_builds_without_a_process_wide_provider() {
        // Guards the `builder_with_provider` choice: a plain `builder()`
        // panics here when no default provider has been installed, and
        // nothing in dollup installs one.
        let _ = client_config();
    }
}
