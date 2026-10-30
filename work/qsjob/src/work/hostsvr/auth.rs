// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Signing on for a host server job, through the storage service: the
//! proof checked as a sign-on, the job then run as the user.

use hostserver::session::{Auth, Proof};
use hostserver::signon::{Change, Info, Refusal, SignonHost, stamp_of};
use hostserver::rmtcmd::JobMessage;
use hostsvr::common::rc;
use sec::signon::Substitute;
use stg::{Client, Shared};

/// Signing on over the job's link.
#[derive(Clone)]
pub struct JobAuth {
    /// The link.
    pub link: Shared,
    /// The job's qualified name.
    pub job: String,
}

fn substitute(proof: Proof<'_>) -> (Substitute, Vec<u8>) {
    match proof {
        Proof::Substitute { seeds, bytes } => (
            Substitute {
                server_seed: seeds.server,
                client_seed: seeds.client,
                substitute: bytes.to_vec(),
            },
            Vec::new(),
        ),
        Proof::Token(t) => (Substitute::default(), t.to_vec()),
    }
}

fn message(id: &str, text: &str) -> JobMessage {
    JobMessage {
        id: id.into(),
        kind: 15,
        severity: 40,
        file: ("QCPFMSG".into(), "QSYS".into()),
        text: text.into(),
        ..JobMessage::default()
    }
}

impl Auth for JobAuth {
    fn password_level(&mut self) -> u8 {
        self.link
            .borrow_mut()
            .with(|l| l.sysval("QPWDLVL".into()))
            .and_then(|v| match v.kind {
                sysapi::SysValKind::Bin => v.data.get(..4).and_then(|b| b.try_into().ok()).map(i32::from_be_bytes),
                sysapi::SysValKind::Char => ccsid::from_ebcdic(&v.data).trim().parse().ok(),
            })
            .and_then(|n| u8::try_from(n).ok())
            .unwrap_or(0)
    }

    fn seed(&mut self) -> [u8; 8] {
        let mut s = [0u8; 8];
        s.copy_from_slice(&sec::password::fresh_salt()[..8]);
        s
    }

    fn start(&mut self, user: &str, proof: Proof<'_>) -> Result<(), u32> {
        let (sub, token) = substitute(proof);
        self.link
            .borrow_mut()
            .with(|l| l.host_signon(user.into(), sub, token, true))
            .map(|_| ())
            .map_err(|e| e.rc)
    }

    fn job_name(&mut self) -> String {
        self.job.clone()
    }
}

impl SignonHost for JobAuth {
    fn verify(&mut self, user: &str, proof: Proof<'_>) -> Result<Info, Refusal> {
        let (sub, token) = substitute(proof);
        let s = self
            .link
            .borrow_mut()
            .with(|l| l.host_signon(user.into(), sub, token, false))
            .map_err(|e| (e.rc, vec![message(&e.msgid, &e.text)]))?;
        Ok(Info {
            current: Some(stamp_of(s.now)),
            last: (s.previous > 0).then(|| stamp_of(s.previous)),
            expires: (s.expires > 0).then(|| stamp_of(s.expires)),
            warning_days: s.warning_days,
            ccsid: 37,
            messages: Vec::new(),
        })
    }

    fn token(&mut self, user: &str, proof: Proof<'_>, kind: u8, timeout: u32) -> Result<Vec<u8>, u32> {
        let (sub, token) = substitute(proof);
        self.link
            .borrow_mut()
            .with(|l| l.host_token(user.into(), sub, token, kind, timeout))
            .map_err(|e| e.rc)
    }

    fn change_password(&mut self, user: &str, c: &Change) -> Result<(), Refusal> {
        let sub = Substitute {
            server_seed: c.seeds.server,
            client_seed: c.seeds.client,
            substitute: c.substitute.clone(),
        };
        self.link
            .borrow_mut()
            .with(|l| l.host_chgpwd(user.into(), sub, c.new_protected.clone(), c.new_len))
            .map_err(|e| {
                let code = if e.rc == 0 { rc::GENERAL } else { e.rc };
                (code, vec![message(&e.msgid, &e.text)])
            })
    }
}
