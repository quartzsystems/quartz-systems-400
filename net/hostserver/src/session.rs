// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What every server but the sign-on server does first: exchange seeds
//! (`0x7001`) and start the server (`0x7002`), which proves the user
//! with a password substitute or a profile token and makes the server
//! job run as that user. Until then nothing else is answered.

use hostsvr::common::{auth, id, rc};
use hostsvr::cp::signon as cp;
use hostsvr::crypt::Seeds;
use hostsvr::frame::{Frame, Param};

use crate::{Outcome, reply, text};

/// How a client proves who it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proof<'a> {
    /// A password substitute over the connection's seeds.
    Substitute {
        /// The seeds.
        seeds: Seeds,
        /// The substitute (8, 20 or 64 bytes).
        bytes: &'a [u8],
    },
    /// A profile token (32 bytes).
    Token(&'a [u8]),
}

/// Signing on, as the server job does it through the storage service.
pub trait Auth {
    /// `QPWDLVL`.
    fn password_level(&mut self) -> u8;
    /// Eight random bytes.
    fn seed(&mut self) -> [u8; 8];
    /// Prove `user` and make the job run as that user (`QSYGETPH` and
    /// `QWTSETP`); an error is a sign-on return code (`common::rc`).
    fn start(&mut self, user: &str, proof: Proof<'_>) -> Result<(), u32>;
    /// The job's qualified name, `number/user/name`.
    fn job_name(&mut self) -> String;
}

/// A connection's sign-on state.
#[derive(Clone, Debug, Default)]
pub struct Session {
    /// The client's seed, once exchanged.
    pub client_seed: Option<[u8; 8]>,
    /// The server's seed.
    pub server_seed: [u8; 8],
    /// The user, once started.
    pub user: Option<String>,
}

/// The user ID parameter: ten EBCDIC bytes (or a CCSID and the name).
pub fn user_of(p: &Param) -> String {
    let d = if p.data.len() > 10 { &p.data[4..] } else { &p.data[..] };
    text::from_ebcdic(d).trim().to_uppercase()
}

/// The job name parameter (`0x111F`): CCSID 0 and the EBCDIC name.
pub fn job_param(job: &str) -> Param {
    let mut d = vec![0, 0, 0, 0];
    d.extend(text::ebcdic(job));
    Param::new(cp::JOB_NAME, d)
}

impl Session {
    /// Whether the server was started.
    pub fn started(&self) -> bool {
        self.user.is_some()
    }

    /// The seeds, once exchanged.
    pub fn seeds(&self) -> Option<Seeds> {
        self.client_seed.map(|client| Seeds {
            client,
            server: self.server_seed,
        })
    }

    /// Answer a request of the sign-on family; `None` for any other.
    pub fn common(&mut self, f: &Frame, a: &mut dyn Auth) -> Option<Outcome> {
        match f.header.req_rep_id {
            id::SEED_EXCHANGE => {
                let mut seed = [0u8; 8];
                seed.copy_from_slice(reply::slice(&f.template, 0, 8));
                self.client_seed = Some(seed);
                self.server_seed = a.seed();
                let mut t = rc::OK.to_be_bytes().to_vec();
                t.extend_from_slice(&self.server_seed);
                Some(Outcome::one(reply::frame(
                    &f.header,
                    id::SEED_EXCHANGE_REPLY,
                    t,
                    Vec::new(),
                )))
            }
            id::START_SERVER => Some(self.start(f, a)),
            id::END_JOB => Some(Outcome::close()),
            _ => None,
        }
    }

    fn start(&mut self, f: &Frame, a: &mut dyn Auth) -> Outcome {
        let scheme = f.template.first().copied().unwrap_or(auth::DES);
        let user = f.param(cp::USER_ID).map(user_of).unwrap_or_default();
        let code = match (scheme, self.seeds()) {
            (auth::PROFILE_TOKEN, _) => match f.param(cp::PROFILE_TOKEN) {
                Some(t) => a.start(&user, Proof::Token(&t.data)),
                None => Err(rc::REQUEST_NOT_VALID),
            },
            (_, Some(seeds)) => match f.param(cp::PASSWORD) {
                Some(p) if !user.is_empty() => a.start(
                    &user,
                    Proof::Substitute {
                        seeds,
                        bytes: &p.data,
                    },
                ),
                Some(_) => Err(rc::USER_ID_LENGTH),
                None => Err(rc::PASSWORD_LENGTH),
            },
            (_, None) => Err(rc::REQUEST_NOT_VALID),
        };
        let rc = match code {
            Ok(()) => {
                self.user = Some(user);
                rc::OK
            }
            Err(e) => e,
        };
        let mut params = Vec::new();
        if rc == rc::OK {
            params.push(job_param(&a.job_name()));
        }
        // The reply goes out only when asked for (send-reply byte 1).
        let send = f.template.get(1).copied().unwrap_or(1) != 0;
        Outcome {
            replies: if send || rc != rc::OK {
                vec![reply::frame(
                    &f.header,
                    id::START_SERVER_REPLY,
                    rc.to_be_bytes().to_vec(),
                    params,
                )]
            } else {
                Vec::new()
            },
            close: rc != rc::OK,
        }
    }
}
