// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sign-on over `netsvr_signon` (1500): the NT hash stays in the service,
//! which counts `QMAXSIGN` and audits. The first profile signed on in
//! the job becomes the job's (the host servers' swap); a later sign-on
//! is only checked, so a sign-on as another profile, which the library's
//! verifier then refuses, never changes whom the job runs as.

use std::sync::{Arc, Mutex, PoisonError};

use netserver::signon::{Refusal, SignOn, SignOnRequest, SignedOn};
use sec::netsvr::{Logon, Refusal as SecRefusal};
use stg::Client;
use stg::netsvr::NsOp;

use super::svc::Svc;

/// `netsvrcfg::Counter` numbers the job counts.
const PASSWORD_VIOLATIONS: u8 = 5;
const UNKNOWN_USERS: u8 = 6;

/// The job's sign-on.
pub struct JobSignOn {
    svc: Arc<Svc>,
    /// The profile the job runs as, once one signed on.
    swapped: Mutex<Option<String>>,
    /// `CPIB691` to `QSYSOPR` at each sign-on.
    log: bool,
}

impl JobSignOn {
    /// Sign-on through `svc`; `log` as the log sign-on attribute.
    pub fn new(svc: Arc<Svc>, log: bool) -> Self {
        Self {
            svc,
            swapped: Mutex::new(None),
            log,
        }
    }
}

fn refusal(r: SecRefusal) -> Refusal {
    match r {
        SecRefusal::NoSuchUser => Refusal::NoSuchUser,
        SecRefusal::Disabled => Refusal::Disabled,
        SecRefusal::Expired => Refusal::Expired,
        SecRefusal::NoPassword => Refusal::NoPassword,
        SecRefusal::NoNtHash => Refusal::NoNtHash,
        SecRefusal::NetServerDisabled => Refusal::NetServerDisabled,
        SecRefusal::WrongPassword { disabled_now } => Refusal::WrongPassword { disabled_now },
        SecRefusal::NotNtlmV2 => Refusal::NotNtlmV2,
    }
}

impl SignOn for JobSignOn {
    fn sign_on(&self, req: &SignOnRequest) -> Result<SignedOn, Refusal> {
        let mut swapped = self.swapped.lock().unwrap_or_else(PoisonError::into_inner);
        let swap = swapped.is_none();
        let logon = Logon {
            user: req.user.clone(),
            domain: req.domain.clone(),
            server_challenge: req.server_challenge,
            nt_response: req.nt_response.clone(),
            guest: req.guest.clone(),
            client: req.client.clone(),
        };
        let user = req.user.clone();
        let log = self.log;
        let r = self
            .svc
            .call(move |link| {
                let r = link.borrow_mut().with(|l| l.netsvr_signon(logon, swap));
                match &r {
                    Ok(ok) if log => super::sysopr(link, "CPIB691", &[&ok.user]),
                    Ok(_) => {}
                    Err(e) => {
                        let mine = refusal(*e);
                        // CPIB682 the service sends itself.
                        if let Some(id) = mine.msgid().filter(|id| *id != "CPIB682") {
                            super::sysopr(link, id, &[&user]);
                        }
                        let counter = match e {
                            SecRefusal::NoSuchUser => UNKNOWN_USERS,
                            _ => PASSWORD_VIOLATIONS,
                        };
                        link.borrow_mut().with(|l| l.netsvr(NsOp::Count { counter, n: 1 }));
                    }
                }
                r
            })
            .ok_or(Refusal::Unavailable)?;
        let ok = r.map_err(refusal)?;
        if swap {
            *swapped = Some(ok.user.clone());
        }
        Ok(SignedOn {
            profile: ok.user,
            guest: ok.guest,
            session_base_key: ok.session_base_key,
        })
    }
}
