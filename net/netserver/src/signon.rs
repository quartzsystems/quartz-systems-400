// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Profile sign-on: the [`SignOn`] trait the job implements over the
//! storage service (which holds the NT hashes and counts `QMAXSIGN`), and
//! [`Verifier`], the library's `LogonVerifier` over it. The types mirror
//! `sec::netsvr` one to one without depending on it.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex, PoisonError};

use smb_server::{Logon, LogonVerifier};
use tracing::{info, warn};

/// What the client sent in its `AUTHENTICATE` message.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SignOnRequest {
    /// The user name as sent.
    pub user: String,
    /// The domain as sent.
    pub domain: String,
    /// The server's challenge.
    pub server_challenge: [u8; 8],
    /// The `NTLMv2` response.
    pub nt_response: Vec<u8>,
    /// The guest profile, when one is configured (`QZLSCHSG`).
    pub guest: Option<String>,
    /// The client's address, for the audit entry.
    pub client: String,
}

/// A sign-on that succeeded.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SignedOn {
    /// The profile the session runs as.
    pub profile: String,
    /// It is the guest profile (an unknown user name).
    pub guest: bool,
    /// The `NTLMv2` session base key; `None` for a guest.
    pub session_base_key: Option<[u8; 16]>,
}

/// Why a sign-on failed: `sec::netsvr::Refusal`'s cases, and one of this
/// crate's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// No profile by the name, and no guest profile.
    #[error("no such user")]
    NoSuchUser,
    /// The profile is `*DISABLED`.
    #[error("profile disabled")]
    Disabled,
    /// The password has expired.
    #[error("password expired")]
    Expired,
    /// `PASSWORD(*NONE)`.
    #[error("no password")]
    NoPassword,
    /// A password without an NT hash.
    #[error("no NetServer password")]
    NoNtHash,
    /// Disabled for `NetServer`.
    #[error("disabled for NetServer")]
    NetServerDisabled,
    /// The response does not prove the password.
    #[error("wrong password")]
    WrongPassword {
        /// This attempt disabled the user for `NetServer`.
        disabled_now: bool,
    },
    /// Not an `NTLMv2` response.
    #[error("not NTLMv2")]
    NotNtlmV2,
    /// Another profile already signed on in this job: a job serves one
    /// profile, since every IFS call runs as it.
    #[error("another profile is signed on in this job")]
    OtherProfile,
    /// The sign-on could not be checked (the service did not answer).
    #[error("sign-on not checked")]
    Unavailable,
}

impl Refusal {
    /// The `QCPFMSG` message `NetServer` logs for it, if any.
    #[must_use]
    pub fn msgid(self) -> Option<&'static str> {
        Some(match self {
            Self::NoSuchUser => "CPIB68A",
            Self::Disabled => "CPIB68B",
            Self::Expired => "CPIB68C",
            Self::NoPassword | Self::NoNtHash => "CPIB68D",
            Self::NetServerDisabled => "CPIB68E",
            Self::WrongPassword { disabled_now: true } => "CPIB682",
            Self::WrongPassword { .. } | Self::NotNtlmV2 | Self::OtherProfile | Self::Unavailable => {
                return None;
            }
        })
    }
}

/// Checks a sign-on. Blocking; called on the connection's task, so keep it
/// to one round trip.
pub trait SignOn: Send + Sync + 'static {
    /// Verifies `req`.
    ///
    /// # Errors
    ///
    /// The [`Refusal`] the service gave.
    fn sign_on(&self, req: &SignOnRequest) -> Result<SignedOn, Refusal>;
}

/// The library's verifier over a [`SignOn`]: fills the guest profile and
/// the client address, binds the job to the first profile signed on and
/// logs every refusal with its reason.
pub struct Verifier {
    signon: Arc<dyn SignOn>,
    guest: Option<String>,
    bound: Arc<Mutex<Option<String>>>,
}

impl Verifier {
    /// A verifier over `signon`, using `guest` for unknown users; `bound`
    /// holds the job's profile once one has signed on.
    pub fn new(signon: Arc<dyn SignOn>, guest: Option<String>, bound: Arc<Mutex<Option<String>>>) -> Self {
        Self { signon, guest, bound }
    }

    /// Runs one sign-on: the service's answer, then the job binding.
    ///
    /// # Errors
    ///
    /// Why it was refused.
    pub fn check(&self, req: &SignOnRequest) -> Result<SignedOn, Refusal> {
        let ok = self.signon.sign_on(req)?;
        let mut bound = self.bound.lock().unwrap_or_else(PoisonError::into_inner);
        match bound.as_deref() {
            Some(p) if !p.eq_ignore_ascii_case(&ok.profile) => Err(Refusal::OtherProfile),
            Some(_) => Ok(ok),
            None => {
                *bound = Some(ok.profile.clone());
                Ok(ok)
            }
        }
    }
}

impl LogonVerifier for Verifier {
    fn verify(&self, user: &str, domain: &str, challenge: &[u8; 8], nt: &[u8]) -> Option<Logon> {
        self.verify_from(None, user, domain, challenge, nt)
    }

    fn verify_from(
        &self,
        peer: Option<SocketAddr>,
        user: &str,
        domain: &str,
        challenge: &[u8; 8],
        nt: &[u8],
    ) -> Option<Logon> {
        let req = SignOnRequest {
            user: user.to_owned(),
            domain: domain.to_owned(),
            server_challenge: *challenge,
            nt_response: nt.to_vec(),
            guest: self.guest.clone(),
            client: peer.map(|p| p.ip().to_string()).unwrap_or_default(),
        };
        match self.check(&req) {
            Ok(s) => {
                info!(user, profile = %s.profile, guest = s.guest, "NetServer sign-on");
                Some(Logon {
                    user: s.profile,
                    session_base_key: s.session_base_key.unwrap_or([0; 16]),
                    guest: s.guest || s.session_base_key.is_none(),
                })
            }
            Err(r) => {
                warn!(user, client = %req.client, reason = %r, msgid = r.msgid().unwrap_or(""), "NetServer sign-on refused");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(Result<SignedOn, Refusal>);

    impl SignOn for Fixed {
        fn sign_on(&self, req: &SignOnRequest) -> Result<SignedOn, Refusal> {
            self.0.clone().map(|mut s| {
                if s.profile.is_empty() {
                    s.profile = req.user.to_uppercase();
                }
                s
            })
        }
    }

    fn verifier(r: Result<SignedOn, Refusal>) -> Verifier {
        Verifier::new(Arc::new(Fixed(r)), Some("QGUEST".into()), Arc::default())
    }

    #[test]
    fn a_refusal_is_no_logon() {
        for r in [Refusal::NoSuchUser, Refusal::WrongPassword { disabled_now: true }, Refusal::NotNtlmV2] {
            assert!(verifier(Err(r)).verify("u", "d", &[0; 8], &[0; 24]).is_none());
        }
        assert_eq!(Refusal::NoNtHash.msgid(), Some("CPIB68D"));
        assert_eq!(Refusal::WrongPassword { disabled_now: true }.msgid(), Some("CPIB682"));
    }

    #[test]
    fn a_guest_has_no_key_and_the_request_names_the_guest_profile() {
        struct Check;
        impl SignOn for Check {
            fn sign_on(&self, req: &SignOnRequest) -> Result<SignedOn, Refusal> {
                assert_eq!(req.guest.as_deref(), Some("QGUEST"));
                assert_eq!(req.client, "10.1.2.3");
                Ok(SignedOn { profile: "QGUEST".into(), guest: true, session_base_key: None })
            }
        }
        let v = Verifier::new(Arc::new(Check), Some("QGUEST".into()), Arc::default());
        let l = v.verify_from(Some("10.1.2.3:5000".parse().unwrap()), "x", "", &[0; 8], &[0; 24]).unwrap();
        assert!(l.guest);
        assert_eq!(l.user, "QGUEST");
    }

    #[test]
    fn the_job_serves_one_profile() {
        let v = verifier(Ok(SignedOn { session_base_key: Some([1; 16]), ..SignedOn::default() }));
        assert_eq!(v.verify("alice", "", &[0; 8], &[0; 24]).unwrap().user, "ALICE");
        assert!(v.verify("Alice", "", &[0; 8], &[0; 24]).is_some());
        assert!(v.verify("bob", "", &[0; 8], &[0; 24]).is_none());
    }
}
