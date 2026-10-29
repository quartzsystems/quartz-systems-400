// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sign-on server (`0xE009`, port 8476): exchange attributes (seeds,
//! versions, the password level), sign-on information, change password,
//! profile tokens. Its replies start with a u32 return code (`common::rc`)
//! and carry messages as `0x112A`/`0x112B`.

use crate::Error;
use crate::cp::signon as cp;
use crate::frame::Param;
use crate::frame::param::find;
use crate::message::HostMessage;
use crate::template::{template, templates};

/// Request and reply IDs.
pub mod id {
    crate::consts! {
        /// Exchange attributes.
        EXCHANGE_ATTRIBUTES = 0x7003,
        /// Its reply.
        EXCHANGE_ATTRIBUTES_REPLY = 0xF003,
        /// Sign-on information (validates the password).
        SIGNON_INFO = 0x7004,
        /// Its reply.
        SIGNON_INFO_REPLY = 0xF004,
        /// Change password.
        CHANGE_PASSWORD = 0x7005,
        /// Its reply.
        CHANGE_PASSWORD_REPLY = 0xF005,
        /// Generate a profile token from a password or token.
        GENERATE_TOKEN = 0x7007,
        /// Its reply (token at frame offset 30, 32 bytes, in `0x1115`).
        GENERATE_TOKEN_REPLY = 0xF007,
        /// Generate a profile token for a user identity.
        GENERATE_TOKEN_IDENTITY = 0x7008,
        /// Its reply.
        GENERATE_TOKEN_IDENTITY_REPLY = 0xF008,
        /// Exchange attributes with the host connection server (`as-hostcnn`).
        HOSTCNN_EXCHANGE_ATTRIBUTES = 0x7103,
        /// Its reply.
        HOSTCNN_EXCHANGE_ATTRIBUTES_REPLY = 0xF103,
    }
}

/// Profile token types (`0x1116`).
pub mod token_type {
    /// Single use.
    pub const SINGLE_USE: u8 = 0xF1;
    /// Multiple use, not regenerable.
    pub const MULTIPLE_USE: u8 = 0xF2;
    /// Multiple use, regenerable.
    pub const REGENERABLE: u8 = 0xF3;
}

template! {
    /// A reply's return code.
    pub struct Rc {
        /// Return code (`common::rc`).
        rc: u32,
    }
}

template! {
    /// A request that names its authentication scheme.
    pub struct AuthRequest {
        /// Authentication scheme (`common::auth`).
        auth_scheme: u8,
    }
}

template! {
    /// Generate a profile token.
    pub struct GenerateTokenRequest {
        /// Authentication scheme (`common::auth`).
        auth_scheme: u8,
        /// Send a reply (0x01).
        send_reply: u8,
    }
}

templates! {
    /// The sign-on server's templates.
    pub enum Template {
        /// `0x7003`.
        ExchangeAttributes(crate::common::Empty) = Request id::EXCHANGE_ATTRIBUTES,
        /// `0xF003`.
        ExchangeAttributesReply(Rc) = Reply id::EXCHANGE_ATTRIBUTES_REPLY,
        /// `0x7004`.
        SignonInfo(AuthRequest) = Request id::SIGNON_INFO,
        /// `0xF004`.
        SignonInfoReply(Rc) = Reply id::SIGNON_INFO_REPLY,
        /// `0x7005`.
        ChangePassword(AuthRequest) = Request id::CHANGE_PASSWORD,
        /// `0xF005`.
        ChangePasswordReply(Rc) = Reply id::CHANGE_PASSWORD_REPLY,
        /// `0x7007`.
        GenerateToken(GenerateTokenRequest) = Request id::GENERATE_TOKEN,
        /// `0xF007`.
        GenerateTokenReply(Rc) = Reply id::GENERATE_TOKEN_REPLY,
        /// `0x7008` (the byte is the token kind, 0x01 profile token).
        GenerateTokenIdentity(AuthRequest) = Request id::GENERATE_TOKEN_IDENTITY,
        /// `0xF008`.
        GenerateTokenIdentityReply(Rc) = Reply id::GENERATE_TOKEN_IDENTITY_REPLY,
        /// `0x7103`.
        HostcnnExchangeAttributes(crate::common::Empty) = Request id::HOSTCNN_EXCHANGE_ATTRIBUTES,
        /// `0xF103`.
        HostcnnExchangeAttributesReply(Rc) = Reply id::HOSTCNN_EXCHANGE_ATTRIBUTES_REPLY,
    }
}

/// Exchange attributes, either way: what the parameters say.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Attributes {
    /// Version (client: 1; server: VRM as `0x00VVRRMM`).
    pub version: Option<u32>,
    /// Datastream level (client asks, server answers).
    pub level: Option<u16>,
    /// The sender's seed.
    pub seed: Option<[u8; 8]>,
    /// The password level (server only).
    pub password_level: Option<u8>,
    /// The server job's name: CCSID and name (server only).
    pub job_name: Option<(u32, Vec<u8>)>,
    /// Additional authentication factor support (server only).
    pub aaf: Option<u8>,
}

impl Attributes {
    /// Read the parameters of an exchange attributes request or reply.
    pub fn from_params(params: &[Param]) -> Result<Self, Error> {
        let mut a = Self::default();
        for p in params {
            match p.cp {
                cp::VERSION => a.version = Some(p.as_u32().ok_or_else(|| bad("version"))?),
                cp::LEVEL => a.level = Some(p.as_u16().ok_or_else(|| bad("level"))?),
                cp::SEED => a.seed = Some(p.data.as_slice().try_into().map_err(|_| bad("seed"))?),
                cp::PASSWORD_LEVEL => {
                    a.password_level = Some(*p.data.first().ok_or_else(|| bad("password level"))?);
                }
                cp::JOB_NAME => a.job_name = Some(ccsid_and(&p.data)?),
                cp::AAF_INDICATOR => a.aaf = p.data.first().copied(),
                _ => {}
            }
        }
        Ok(a)
    }

    /// The parameters, in the order servers send them.
    pub fn to_params(&self) -> Vec<Param> {
        let mut v = Vec::new();
        if let Some(x) = self.version {
            v.push(Param::u32(cp::VERSION, x));
        }
        if let Some(x) = self.level {
            v.push(Param::u16(cp::LEVEL, x));
        }
        if let Some(x) = self.seed {
            v.push(Param::new(cp::SEED, x));
        }
        if let Some(x) = self.password_level {
            v.push(Param::byte(cp::PASSWORD_LEVEL, x));
        }
        if let Some((c, n)) = &self.job_name {
            let mut d = c.to_be_bytes().to_vec();
            d.extend_from_slice(n);
            v.push(Param::new(cp::JOB_NAME, d));
        }
        if let Some(x) = self.aaf {
            v.push(Param::byte(cp::AAF_INDICATOR, x));
        }
        v
    }
}

/// A date in a sign-on information reply: year (u16), month, day, hour,
/// minute, second, then bytes this codec keeps as they are.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SignonDate {
    /// Year.
    pub year: u16,
    /// Month, 1 to 12.
    pub month: u8,
    /// Day.
    pub day: u8,
    /// Hour.
    pub hour: u8,
    /// Minute.
    pub minute: u8,
    /// Second.
    pub second: u8,
    /// Anything after the second.
    pub rest: Vec<u8>,
}

impl SignonDate {
    /// Decode a date parameter's data.
    pub fn decode(b: &[u8]) -> Result<Self, Error> {
        if b.len() < 7 {
            return Err(bad("date"));
        }
        Ok(Self {
            year: u16::from_be_bytes([b[0], b[1]]),
            month: b[2],
            day: b[3],
            hour: b[4],
            minute: b[5],
            second: b[6],
            rest: b[7..].to_vec(),
        })
    }

    /// The data.
    pub fn encode(&self) -> Vec<u8> {
        let mut v = self.year.to_be_bytes().to_vec();
        v.extend_from_slice(&[self.month, self.day, self.hour, self.minute, self.second]);
        v.extend_from_slice(&self.rest);
        v
    }
}

/// What a sign-on information reply says.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SignonInfo {
    /// This sign-on.
    pub current: Option<SignonDate>,
    /// The previous sign-on.
    pub last: Option<SignonDate>,
    /// Password expiration.
    pub expiration: Option<SignonDate>,
    /// Password expiration warning days.
    pub warning_days: Option<u32>,
    /// The server CCSID.
    pub server_ccsid: Option<u32>,
    /// The user: CCSID and name.
    pub user: Option<(u32, Vec<u8>)>,
    /// Messages.
    pub messages: Vec<HostMessage>,
}

impl SignonInfo {
    /// Read a sign-on information reply's parameters.
    pub fn from_params(params: &[Param]) -> Result<Self, Error> {
        let date = |c| {
            find(params, c)
                .map(|p| SignonDate::decode(&p.data))
                .transpose()
        };
        Ok(Self {
            current: date(cp::CURRENT_SIGNON_DATE)?,
            last: date(cp::LAST_SIGNON_DATE)?,
            expiration: date(cp::EXPIRATION_DATE)?,
            warning_days: find(params, cp::WARNING_DAYS).and_then(Param::as_u32),
            server_ccsid: find(params, cp::SERVER_CCSID).and_then(Param::as_u32),
            user: find(params, cp::USER_ID)
                .map(|p| ccsid_and(&p.data))
                .transpose()?,
            messages: messages(params)?,
        })
    }
}

/// The messages of a sign-on family reply (`0x112B`).
pub fn messages(params: &[Param]) -> Result<Vec<HostMessage>, Error> {
    params
        .iter()
        .filter(|p| p.cp == cp::MESSAGE)
        .map(|p| HostMessage::decode(&p.data))
        .collect()
}

/// A CCSID (u32) and the bytes after it.
fn ccsid_and(b: &[u8]) -> Result<(u32, Vec<u8>), Error> {
    if b.len() < 4 {
        return Err(bad("CCSID-prefixed field"));
    }
    Ok((
        u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        b[4..].to_vec(),
    ))
}

fn bad(what: &str) -> Error {
    Error::Invalid(format!("sign-on {what} has the wrong size"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attributes_round_trip() {
        let a = Attributes {
            version: Some(0x0007_0100),
            level: Some(10),
            seed: Some([1; 8]),
            password_level: Some(2),
            job_name: Some((37, b"x".to_vec())),
            aaf: None,
        };
        assert_eq!(Attributes::from_params(&a.to_params()).unwrap(), a);
    }
}
