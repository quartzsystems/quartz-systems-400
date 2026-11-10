//! QS/400 addition: logons checked by the application instead of the
//! server's own user table (see `QS400-PATCHES.md`).

/// A logon the verifier accepted.
#[derive(Debug, Clone)]
pub struct Logon {
    /// The user name the session carries (share grants match it).
    pub user: String,
    /// The NTLMv2 session base key.
    pub session_base_key: [u8; 16],
}

/// Checks NTLMv2 responses for the server.
///
/// Called during `SESSION_SETUP` with the user and domain as the client sent
/// them, the server challenge and the client's NT response. Returning `None`
/// fails the logon with `STATUS_LOGON_FAILURE`. The call blocks the
/// connection's task; keep it short.
pub trait LogonVerifier: Send + Sync + 'static {
    /// Verifies one response.
    fn verify(
        &self,
        user: &str,
        domain: &str,
        server_challenge: &[u8; 8],
        nt_response: &[u8],
    ) -> Option<Logon>;
}

/// A shared verifier, as the server's configuration holds it.
#[derive(Clone)]
pub(crate) struct VerifierRef(pub(crate) std::sync::Arc<dyn LogonVerifier>);

impl std::fmt::Debug for VerifierRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LogonVerifier")
    }
}

impl std::ops::Deref for VerifierRef {
    type Target = dyn LogonVerifier;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// The share grant key meaning every signed-on user ([`crate::Share::any_user`]).
pub(crate) const ANY_USER: &str = "*";
