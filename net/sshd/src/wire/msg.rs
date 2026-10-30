// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Message numbers (RFC 4250 section 4.1) and the reason codes.

/// `SSH_MSG_DISCONNECT`.
pub const DISCONNECT: u8 = 1;
/// `SSH_MSG_IGNORE`.
pub const IGNORE: u8 = 2;
/// `SSH_MSG_UNIMPLEMENTED`.
pub const UNIMPLEMENTED: u8 = 3;
/// `SSH_MSG_DEBUG`.
pub const DEBUG: u8 = 4;
/// `SSH_MSG_SERVICE_REQUEST`.
pub const SERVICE_REQUEST: u8 = 5;
/// `SSH_MSG_SERVICE_ACCEPT`.
pub const SERVICE_ACCEPT: u8 = 6;
/// `SSH_MSG_EXT_INFO` (RFC 8308).
pub const EXT_INFO: u8 = 7;
/// `SSH_MSG_KEXINIT`.
pub const KEXINIT: u8 = 20;
/// `SSH_MSG_NEWKEYS`.
pub const NEWKEYS: u8 = 21;
/// `SSH_MSG_KEX_ECDH_INIT` (also the hybrid KEMs' init).
pub const KEX_ECDH_INIT: u8 = 30;
/// `SSH_MSG_KEX_ECDH_REPLY`.
pub const KEX_ECDH_REPLY: u8 = 31;
/// `SSH_MSG_USERAUTH_REQUEST`.
pub const USERAUTH_REQUEST: u8 = 50;
/// `SSH_MSG_USERAUTH_FAILURE`.
pub const USERAUTH_FAILURE: u8 = 51;
/// `SSH_MSG_USERAUTH_SUCCESS`.
pub const USERAUTH_SUCCESS: u8 = 52;
/// `SSH_MSG_USERAUTH_BANNER`.
pub const USERAUTH_BANNER: u8 = 53;
/// `SSH_MSG_USERAUTH_PK_OK` (and `PASSWD_CHANGEREQ`).
pub const USERAUTH_PK_OK: u8 = 60;
/// `SSH_MSG_GLOBAL_REQUEST`.
pub const GLOBAL_REQUEST: u8 = 80;
/// `SSH_MSG_REQUEST_SUCCESS`.
pub const REQUEST_SUCCESS: u8 = 81;
/// `SSH_MSG_REQUEST_FAILURE`.
pub const REQUEST_FAILURE: u8 = 82;
/// `SSH_MSG_CHANNEL_OPEN`.
pub const CHANNEL_OPEN: u8 = 90;
/// `SSH_MSG_CHANNEL_OPEN_CONFIRMATION`.
pub const CHANNEL_OPEN_CONFIRMATION: u8 = 91;
/// `SSH_MSG_CHANNEL_OPEN_FAILURE`.
pub const CHANNEL_OPEN_FAILURE: u8 = 92;
/// `SSH_MSG_CHANNEL_WINDOW_ADJUST`.
pub const CHANNEL_WINDOW_ADJUST: u8 = 93;
/// `SSH_MSG_CHANNEL_DATA`.
pub const CHANNEL_DATA: u8 = 94;
/// `SSH_MSG_CHANNEL_EXTENDED_DATA`.
pub const CHANNEL_EXTENDED_DATA: u8 = 95;
/// `SSH_MSG_CHANNEL_EOF`.
pub const CHANNEL_EOF: u8 = 96;
/// `SSH_MSG_CHANNEL_CLOSE`.
pub const CHANNEL_CLOSE: u8 = 97;
/// `SSH_MSG_CHANNEL_REQUEST`.
pub const CHANNEL_REQUEST: u8 = 98;
/// `SSH_MSG_CHANNEL_SUCCESS`.
pub const CHANNEL_SUCCESS: u8 = 99;
/// `SSH_MSG_CHANNEL_FAILURE`.
pub const CHANNEL_FAILURE: u8 = 100;

/// Disconnect reason codes (RFC 4250 section 4.2.2).
pub mod reason {
    /// `SSH_DISCONNECT_PROTOCOL_ERROR`.
    pub const PROTOCOL_ERROR: u32 = 2;
    /// `SSH_DISCONNECT_KEY_EXCHANGE_FAILED`.
    pub const KEY_EXCHANGE_FAILED: u32 = 3;
    /// `SSH_DISCONNECT_MAC_ERROR`.
    pub const MAC_ERROR: u32 = 5;
    /// `SSH_DISCONNECT_SERVICE_NOT_AVAILABLE`.
    pub const SERVICE_NOT_AVAILABLE: u32 = 7;
    /// `SSH_DISCONNECT_PROTOCOL_VERSION_NOT_SUPPORTED`.
    pub const VERSION_NOT_SUPPORTED: u32 = 8;
    /// `SSH_DISCONNECT_BY_APPLICATION`.
    pub const BY_APPLICATION: u32 = 11;
    /// `SSH_DISCONNECT_TOO_MANY_CONNECTIONS`.
    pub const TOO_MANY_CONNECTIONS: u32 = 12;
    /// `SSH_DISCONNECT_NO_MORE_AUTH_METHODS_AVAILABLE`.
    pub const NO_MORE_AUTH_METHODS: u32 = 14;
}

/// Channel open failure codes (RFC 4250 section 4.3).
pub mod open_failure {
    /// `SSH_OPEN_ADMINISTRATIVELY_PROHIBITED`.
    pub const PROHIBITED: u32 = 1;
    /// `SSH_OPEN_CONNECT_FAILED`.
    pub const CONNECT_FAILED: u32 = 2;
    /// `SSH_OPEN_UNKNOWN_CHANNEL_TYPE`.
    pub const UNKNOWN_CHANNEL_TYPE: u32 = 3;
    /// `SSH_OPEN_RESOURCE_SHORTAGE`.
    pub const RESOURCE_SHORTAGE: u32 = 4;
}

/// `SSH_EXTENDED_DATA_STDERR`.
pub const STDERR: u32 = 1;
