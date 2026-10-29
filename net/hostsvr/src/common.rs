// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What every server shares: the random seed exchange, the start-server
//! request that authenticates the connection, and end job. Byte 4 of the
//! header carries client attributes in these requests (`0x03` in the
//! seed exchange: SHA-1 and level 4 encryption and additional
//! authentication factors supported; `0x02` in start server: return job
//! information).

use crate::template::{template, templates};

/// Request and reply IDs.
pub mod id {
    crate::consts! {
        /// Exchange random seeds.
        SEED_EXCHANGE = 0x7001,
        /// Its reply.
        SEED_EXCHANGE_REPLY = 0xF001,
        /// Start server (authenticate).
        START_SERVER = 0x7002,
        /// Its reply.
        START_SERVER_REPLY = 0xF002,
        /// End the server job (sent before close; no reply).
        END_JOB = 0x7006,
    }
}

/// Authentication schemes (template byte 0 of start server, sign-on
/// information, change password and generate token).
pub mod auth {
    /// DES password substitute (8 bytes; password levels 0 and 1).
    pub const DES: u8 = 0x01;
    /// Profile token.
    pub const PROFILE_TOKEN: u8 = 0x02;
    /// SHA-1 password substitute (20 bytes; levels 2 and 3).
    pub const SHA1: u8 = 0x03;
    /// GSS (Kerberos) token.
    pub const GSS_TOKEN: u8 = 0x05;
    /// Identity token.
    pub const IDENTITY_TOKEN: u8 = 0x06;
    /// SHA-512 password substitute (64 bytes; level 4).
    pub const SHA512: u8 = 0x07;

    /// The scheme for a password substitute of `len` bytes.
    pub fn for_substitute(len: usize) -> u8 {
        match len {
            8 => DES,
            20 => SHA1,
            _ => SHA512,
        }
    }
}

/// Header byte 4 values (client attributes).
pub mod attributes {
    /// Seed exchange: SHA-1, level 4 and additional factors supported.
    pub const SEED_EXCHANGE: u8 = 0x03;
    /// Start server: return job information.
    pub const START_SERVER: u8 = 0x02;
}

/// Return codes of the sign-on family replies (u32). The high half is
/// the class: 1 request not valid, 2 user ID, 3 password, 4 general.
pub mod rc {
    /// Success.
    pub const OK: u32 = 0;
    /// User ID length not valid (`0x00010001` to `0x00010007`).
    pub const USER_ID_LENGTH: u32 = 0x0001_0001;
    /// Password length not valid.
    pub const PASSWORD_LENGTH: u32 = 0x0001_0008;
    /// Sign-on request not valid (`0x00010009` to `0x0001000B`).
    pub const REQUEST_NOT_VALID: u32 = 0x0001_0009;
    /// Change password request not valid.
    pub const CHANGE_REQUEST_NOT_VALID: u32 = 0x0001_000C;
    /// Old password not valid.
    pub const OLD_PASSWORD_NOT_VALID: u32 = 0x0001_000D;
    /// New password not valid.
    pub const NEW_PASSWORD_NOT_VALID: u32 = 0x0001_000E;
    /// Token type not valid.
    pub const TOKEN_TYPE: u32 = 0x0001_000F;
    /// Generate token request not valid.
    pub const TOKEN_REQUEST: u32 = 0x0001_0010;
    /// Token length not valid.
    pub const TOKEN_LENGTH: u32 = 0x0001_0011;
    /// The user ID is unknown.
    pub const USER_UNKNOWN: u32 = 0x0002_0001;
    /// The user profile is disabled.
    pub const USER_DISABLED: u32 = 0x0002_0002;
    /// The user ID does not match.
    pub const USER_MISMATCH: u32 = 0x0002_0003;
    /// New password too long (`0x00030001`; up to `0x0003000A` the other
    /// password rules: too short, repeated or adjacent characters, used
    /// before, no digit, character not valid, disallowed, contains user).
    pub const NEW_PASSWORD_TOO_LONG: u32 = 0x0003_0001;
    /// The password is not correct.
    pub const PASSWORD_INCORRECT: u32 = 0x0003_000B;
    /// The password is not correct and the profile is now disabled.
    pub const PASSWORD_INCORRECT_DISABLED: u32 = 0x0003_000C;
    /// The password is expired.
    pub const PASSWORD_EXPIRED: u32 = 0x0003_000D;
    /// The password is *NONE.
    pub const PASSWORD_NONE: u32 = 0x0003_0010;
    /// General security error.
    pub const GENERAL: u32 = 0x0004_0000;
}

template! {
    /// Exchange random seeds (`0x7001`).
    pub struct SeedRequest {
        /// The client's seed.
        client_seed: [u8; 8],
    }
}

template! {
    /// Exchange random seeds reply (`0xF001`).
    pub struct SeedReply {
        /// Return code.
        rc: u32,
        /// The server's seed.
        server_seed: [u8; 8],
    }
}

template! {
    /// Start server (`0x7002`): password or token in `0x1105`/`0x1115`,
    /// user in `0x1104`.
    pub struct StartServerRequest {
        /// Authentication scheme (`auth`).
        auth_scheme: u8,
        /// Send a reply (0x01).
        send_reply: u8,
    }
}

template! {
    /// Start server reply (`0xF002`): user in `0x1104`, job in `0x111F`.
    pub struct StartServerReply {
        /// Return code.
        rc: u32,
    }
}

template! {
    /// A template with no bytes.
    pub struct Empty {}
}

templates! {
    /// The shared templates.
    pub enum Template {
        /// `0x7001`.
        SeedRequest(SeedRequest) = Request id::SEED_EXCHANGE,
        /// `0xF001`.
        SeedReply(SeedReply) = Reply id::SEED_EXCHANGE_REPLY,
        /// `0x7002`.
        StartServer(StartServerRequest) = Request id::START_SERVER,
        /// `0xF002`.
        StartServerReply(StartServerReply) = Reply id::START_SERVER_REPLY,
        /// `0x7006`.
        EndJob(Empty) = Request id::END_JOB,
    }
}
