// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sign-on server (8476): exchange attributes (versions, seeds, the
//! password level), sign-on information (validates the password and
//! answers the sign-on dates), change password, profile tokens.

use hostsvr::common::{auth, rc};
use hostsvr::cp::signon as cp;
use hostsvr::crypt::Seeds;
use hostsvr::frame::{Frame, Param};
use hostsvr::signon::{Attributes, id};
use regfac::ExitRequest;
use regfac::formats::host::{func, zsoy0100};

use crate::exits::{Exits, who};
use crate::rmtcmd::JobMessage;
use crate::session::{Auth, Proof, user_of};
use crate::{Handler, Outcome, SERVER_LEVEL, SERVER_VRM, reply};

/// The exit point.
pub const EXIT_POINT: &str = "QIBM_QZSO_SIGNONSRV";

/// A date and time: year, month, day, hour, minute, second.
pub type Stamp = (u16, u8, u8, u8, u8, u8);

/// A Unix time as a sign-on date (UTC).
#[allow(clippy::many_single_char_names)]
pub fn stamp_of(secs: u64) -> Stamp {
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let b = |v: u64| u8::try_from(v).unwrap_or(0);
    (
        u16::try_from(y).unwrap_or(1970),
        u8::try_from(m).unwrap_or(1),
        u8::try_from(d).unwrap_or(1),
        b(rem / 3600),
        b(rem % 3600 / 60),
        b(rem % 60),
    )
}

/// What sign-on information answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Info {
    /// This sign-on.
    pub current: Option<Stamp>,
    /// The previous sign-on.
    pub last: Option<Stamp>,
    /// When the password expires.
    pub expires: Option<Stamp>,
    /// Days of warning before expiry (`QPWDEXPWRN`).
    pub warning_days: u32,
    /// The server job's CCSID.
    pub ccsid: u32,
    /// Messages (a password about to expire, say).
    pub messages: Vec<JobMessage>,
}

/// A change password request as sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The seeds.
    pub seeds: Seeds,
    /// The old password's substitute.
    pub substitute: Vec<u8>,
    /// The new password protected with the old token (`0x110D`).
    pub new_protected: Vec<u8>,
    /// The old password protected with the new token (`0x110C`).
    pub old_protected: Vec<u8>,
    /// The new password's length in bytes (SHA forms).
    pub new_len: u32,
    /// The old password's length (SHA forms).
    pub old_len: u32,
}

/// A failure: a sign-on return code and messages.
pub type Refusal = (u32, Vec<JobMessage>);

/// What the sign-on server asks of the system beyond [`Auth`].
pub trait SignonHost: Auth {
    /// Validate `user` (counting a failure against `QMAXSIGN`) without
    /// becoming that user.
    fn verify(&mut self, user: &str, proof: Proof<'_>) -> Result<Info, Refusal>;
    /// A profile token for `user`.
    fn token(&mut self, user: &str, proof: Proof<'_>, kind: u8, timeout: u32) -> Result<Vec<u8>, u32>;
    /// Change `user`'s password.
    fn change_password(&mut self, user: &str, c: &Change) -> Result<(), Refusal>;
}

/// The server.
pub struct Signon<H, X> {
    /// The system.
    pub host: H,
    /// Exit programs.
    pub exits: X,
    client_seed: Option<[u8; 8]>,
    server_seed: [u8; 8],
}

fn stamp(cp: u16, s: Stamp) -> Param {
    let mut d = s.0.to_be_bytes().to_vec();
    d.extend([s.1, s.2, s.3, s.4, s.5]);
    Param::new(cp, d)
}

fn messages(msgs: &[JobMessage]) -> Vec<Param> {
    if msgs.is_empty() {
        return Vec::new();
    }
    let mut v = vec![Param::u16(cp::MESSAGE_COUNT, reply::len16(msgs.len()))];
    v.extend(msgs.iter().map(|m| Param::new(cp::MESSAGE, m.to_host().encode())));
    v
}

fn user_param(user: &str) -> Param {
    let mut d = 37u32.to_be_bytes().to_vec();
    d.extend(crate::text::ebcdic_fixed(user, 10));
    Param::new(cp::USER_ID, d)
}

impl<H: SignonHost, X: Exits> Signon<H, X> {
    /// A server for one connection.
    pub fn new(host: H, exits: X) -> Self {
        Self {
            host,
            exits,
            client_seed: None,
            server_seed: [0; 8],
        }
    }

    fn seeds(&self) -> Option<Seeds> {
        self.client_seed.map(|client| Seeds {
            client,
            server: self.server_seed,
        })
    }

    fn exchange(&mut self, f: &Frame) -> Outcome {
        let a = Attributes::from_params(&f.params).unwrap_or_default();
        self.client_seed = a.seed;
        self.server_seed = self.host.seed();
        let attrs = Attributes {
            version: Some(SERVER_VRM),
            level: Some(SERVER_LEVEL),
            seed: Some(self.server_seed),
            password_level: Some(self.host.password_level()),
            job_name: Some((0, crate::text::ebcdic(&self.host.job_name()))),
            aaf: None,
        };
        Outcome::one(reply::frame(
            &f.header,
            id::EXCHANGE_ATTRIBUTES_REPLY,
            rc::OK.to_be_bytes().to_vec(),
            attrs.to_params(),
        ))
    }

    /// The user and proof of a request.
    fn proof<'a>(&self, f: &'a Frame) -> Result<(String, Proof<'a>), u32> {
        let user = f.param(cp::USER_ID).map(user_of).unwrap_or_default();
        let scheme = f.template.first().copied().unwrap_or(auth::DES);
        if scheme == auth::PROFILE_TOKEN {
            let t = f.param(cp::PROFILE_TOKEN).ok_or(rc::TOKEN_LENGTH)?;
            return Ok((user, Proof::Token(&t.data)));
        }
        let seeds = self.seeds().ok_or(rc::REQUEST_NOT_VALID)?;
        let p = f.param(cp::PASSWORD).ok_or(rc::PASSWORD_LENGTH)?;
        if user.is_empty() {
            return Err(rc::USER_ID_LENGTH);
        }
        Ok((
            user,
            Proof::Substitute {
                seeds,
                bytes: &p.data,
            },
        ))
    }

    fn allowed(&mut self, user: &str, function: i32) -> bool {
        let req = ExitRequest::host("ZSOY0100", zsoy0100(&who(user, function)));
        self.exits.allow(EXIT_POINT, &req)
    }

    fn info(&mut self, f: &Frame) -> Outcome {
        let rid = id::SIGNON_INFO_REPLY;
        let (user, proof) = match self.proof(f) {
            Ok(x) => x,
            Err(code) => return Outcome::one(reply::frame(&f.header, rid, code.to_be_bytes().to_vec(), Vec::new())),
        };
        if !self.allowed(&user, func::RETRIEVE_SIGNON) {
            return Outcome::one(reply::frame(&f.header, rid, rc::GENERAL.to_be_bytes().to_vec(), Vec::new()));
        }
        let want_msgs = f.param(cp::RETURN_MESSAGES).is_some_and(|p| p.data.first() == Some(&1));
        match self.host.verify(&user, proof) {
            Ok(i) => {
                let mut params = Vec::new();
                if let Some(s) = i.current {
                    params.push(stamp(cp::CURRENT_SIGNON_DATE, s));
                }
                if let Some(s) = i.last {
                    params.push(stamp(cp::LAST_SIGNON_DATE, s));
                }
                if let Some(s) = i.expires {
                    params.push(stamp(cp::EXPIRATION_DATE, s));
                }
                params.push(Param::u32(cp::WARNING_DAYS, i.warning_days));
                params.push(Param::u32(cp::SERVER_CCSID, i.ccsid));
                params.push(user_param(&user));
                if want_msgs {
                    params.extend(messages(&i.messages));
                }
                Outcome::one(reply::frame(&f.header, rid, rc::OK.to_be_bytes().to_vec(), params))
            }
            Err((code, msgs)) => {
                let params = if want_msgs { messages(&msgs) } else { Vec::new() };
                Outcome::one(reply::frame(&f.header, rid, code.to_be_bytes().to_vec(), params))
            }
        }
    }

    fn change(&mut self, f: &Frame) -> Outcome {
        let rid = id::CHANGE_PASSWORD_REPLY;
        let user = f.param(cp::USER_ID).map(user_of).unwrap_or_default();
        let get = |c| f.param(c).map(|p| p.data.clone()).unwrap_or_default();
        let Some(seeds) = self.seeds() else {
            return Outcome::one(reply::frame(&f.header, rid, rc::REQUEST_NOT_VALID.to_be_bytes().to_vec(), Vec::new()));
        };
        let c = Change {
            seeds,
            substitute: get(cp::PASSWORD),
            new_protected: get(cp::NEW_PASSWORD),
            old_protected: get(cp::OLD_PASSWORD),
            new_len: f.param(cp::NEW_PASSWORD_LENGTH).and_then(Param::as_u32).unwrap_or(0),
            old_len: f.param(cp::OLD_PASSWORD_LENGTH).and_then(Param::as_u32).unwrap_or(0),
        };
        if !self.allowed(&user, func::CHANGE_PASSWORD) {
            return Outcome::one(reply::frame(&f.header, rid, rc::GENERAL.to_be_bytes().to_vec(), Vec::new()));
        }
        let want_msgs = f.param(cp::RETURN_MESSAGES).is_some_and(|p| p.data.first() == Some(&1));
        let (code, msgs) = match self.host.change_password(&user, &c) {
            Ok(()) => (rc::OK, Vec::new()),
            Err(r) => r,
        };
        let params = if want_msgs { messages(&msgs) } else { Vec::new() };
        Outcome::one(reply::frame(&f.header, rid, code.to_be_bytes().to_vec(), params))
    }

    fn token(&mut self, f: &Frame, rid: u16) -> Outcome {
        let fail = |code: u32| Outcome::one(reply::frame(&f.header, rid, code.to_be_bytes().to_vec(), Vec::new()));
        let (user, proof) = match self.proof(f) {
            Ok(x) => x,
            Err(code) => return fail(code),
        };
        if !self.allowed(&user, func::GENERATE_TOKEN) {
            return fail(rc::GENERAL);
        }
        let kind = f
            .param(cp::TOKEN_TYPE)
            .and_then(|p| p.data.first().copied())
            .unwrap_or(hostsvr::signon::token_type::SINGLE_USE);
        let timeout = f.param(cp::TOKEN_TIMEOUT).and_then(Param::as_u32).unwrap_or(3600);
        match self.host.token(&user, proof, kind, timeout) {
            Ok(t) => Outcome::one(reply::frame(
                &f.header,
                rid,
                rc::OK.to_be_bytes().to_vec(),
                vec![Param::new(cp::PROFILE_TOKEN, t)],
            )),
            Err(code) => fail(code),
        }
    }
}

impl<H: SignonHost, X: Exits> Handler for Signon<H, X> {
    fn handle(&mut self, frame: &[u8]) -> Outcome {
        let Ok(f) = Frame::decode(frame) else {
            return Outcome::close();
        };
        match f.header.req_rep_id {
            id::EXCHANGE_ATTRIBUTES | id::HOSTCNN_EXCHANGE_ATTRIBUTES => self.exchange(&f),
            id::SIGNON_INFO => self.info(&f),
            id::CHANGE_PASSWORD => self.change(&f),
            id::GENERATE_TOKEN => self.token(&f, id::GENERATE_TOKEN_REPLY),
            id::GENERATE_TOKEN_IDENTITY => self.token(&f, id::GENERATE_TOKEN_IDENTITY_REPLY),
            _ => Outcome::close(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps() {
        assert_eq!(stamp_of(0), (1970, 1, 1, 0, 0, 0));
        assert_eq!(stamp_of(1_792_672_051), (2026, 10, 22, 12, 27, 31));
    }
}
