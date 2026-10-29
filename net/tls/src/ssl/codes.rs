// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The numbers of `qsossl.h` and Quartz text for the return codes.

#![allow(missing_docs)]

// SSL_Create flags.
pub const SSL_ENCRYPT_MASK: i32 = 1;
pub const SSL_ENCRYPT: i32 = 1;
pub const SSL_DONT_ENCRYPT: i32 = 0;

// SSL_Handshake `how`.
pub const SSL_HANDSHAKE_AS_CLIENT: i32 = 0;
pub const SSL_HANDSHAKE_AS_SERVER: i32 = 1;
pub const SSL_HANDSHAKE_AS_SERVER_WITH_CLIENT_AUTH: i32 = 2;
pub const SSL_HANDSHAKE_AS_SERVER_WITH_OPTIONAL_CLIENT_AUTH: i32 = 3;

// sessionType.
pub const SSL_REGISTERED_AS_CLIENT: u32 = 0;
pub const SSL_REGISTERED_AS_SERVER: u32 = 1;
pub const SSL_REGISTERED_AS_SERVER_WITH_CLIENT_AUTH: u32 = 2;
pub const SSL_REGISTERED_AS_SERVER_WITH_OPTIONAL_CLIENT_AUTH: u32 = 3;
pub const SSL_REGISTERED_AS_NOT_SPECIFIED: u32 = 99;

// protocol.
pub const SSL_VERSION_CURRENT: u32 = 0;
pub const SSL_VERSION_2: u32 = 2;
pub const SSL_VERSION_3: u32 = 3;
pub const TLS_VERSION_1: u32 = 4;
pub const TLSV1_SSLV3: u32 = 5;
pub const TLS_VERSION_1_0: u32 = 6;
pub const TLS_VERSION_1_1: u32 = 7;
pub const TLS_VERSION_1_2: u32 = 8;
pub const TLS_VERSION_1_3: u32 = 9;

// timeout.
pub const SSL_TIMEOUT_DISABLE: u32 = 0xFFFF_FFFF;
pub const SSL_TIMEOUT_DEFAULT: u32 = 0;
pub const SSL_TIMEOUT_MAX: u32 = 86_400;

// Return codes.
pub const SSL_ERROR_NO_CIPHERS: i32 = -1;
pub const SSL_ERROR_NO_CERTIFICATE: i32 = -2;
pub const SSL_ERROR_BAD_CERTIFICATE: i32 = -4;
pub const SSL_ERROR_UNSUPPORTED_CERTIFICATE_TYPE: i32 = -6;
pub const SSL_ERROR_IO: i32 = -10;
pub const SSL_ERROR_BAD_MESSAGE: i32 = -11;
pub const SSL_ERROR_BAD_MAC: i32 = -12;
pub const SSL_ERROR_UNSUPPORTED: i32 = -13;
pub const SSL_ERROR_BAD_CERT_SIG: i32 = -14;
pub const SSL_ERROR_BAD_CERT: i32 = -15;
pub const SSL_ERROR_BAD_PEER: i32 = -16;
pub const SSL_ERROR_PERMISSION_DENIED: i32 = -17;
pub const SSL_ERROR_SELF_SIGNED: i32 = -18;
pub const SSL_ERROR_BAD_MALLOC: i32 = -20;
pub const SSL_ERROR_BAD_STATE: i32 = -21;
pub const SSL_ERROR_SOCKET_CLOSED: i32 = -22;
pub const SSL_ERROR_NOT_TRUSTED_ROOT: i32 = -23;
pub const SSL_ERROR_CERT_EXPIRED: i32 = -24;
pub const SSL_ERROR_BAD_DATE: i32 = -25;
pub const SSL_ERROR_BAD_KEY_LEN_FOR_EXPORT: i32 = -26;
pub const SSL_ERROR_NOT_KEYRING: i32 = -90;
pub const SSL_ERROR_KEYPASSWORD_EXPIRED: i32 = -91;
pub const SSL_ERROR_CERTIFICATE_REJECTED: i32 = -92;
pub const SSL_ERROR_SSL_NOT_AVAILABLE: i32 = -93;
pub const SSL_ERROR_NO_INIT: i32 = -94;
pub const SSL_ERROR_NO_KEYRING: i32 = -95;
pub const SSL_ERROR_NOT_ENABLED: i32 = -96;
pub const SSL_ERROR_BAD_CIPHER_SUITE: i32 = -97;
pub const SSL_ERROR_CLOSED: i32 = -98;
pub const SSL_ERROR_UNKNOWN: i32 = -99;
pub const SSL_ERROR_NOT_REGISTERED: i32 = -1009;
pub const SSL_ERROR_NO_CERTIFICATE_AUTHORITIES: i32 = -1011;

/// The text of a return code.
pub fn text(rc: i32) -> &'static str {
    match rc {
        0 => "The operation completed.",
        SSL_ERROR_NO_CIPHERS => "The two sides share no protocol version or cipher suite.",
        SSL_ERROR_NO_CERTIFICATE => "No certificate is available for this session.",
        SSL_ERROR_BAD_CERTIFICATE => "The certificate is not valid.",
        SSL_ERROR_UNSUPPORTED_CERTIFICATE_TYPE => "The certificate type is not supported.",
        SSL_ERROR_IO => "The transport failed during a secure operation.",
        SSL_ERROR_BAD_MESSAGE => "The peer sent a message that is not valid.",
        SSL_ERROR_BAD_MAC => "A record failed its integrity check.",
        SSL_ERROR_UNSUPPORTED => "The protocol version asked for is not supported.",
        SSL_ERROR_BAD_CERT_SIG => "The certificate's signature is not valid.",
        SSL_ERROR_BAD_CERT => "The certificate is damaged.",
        SSL_ERROR_BAD_PEER => "The peer's identity could not be accepted.",
        SSL_ERROR_PERMISSION_DENIED => "The operation is not permitted.",
        SSL_ERROR_SELF_SIGNED => "The peer's certificate is self-signed and not trusted.",
        SSL_ERROR_BAD_MALLOC => "Storage could not be obtained.",
        SSL_ERROR_BAD_STATE => "The handle is not in the state this operation needs.",
        SSL_ERROR_SOCKET_CLOSED => "The peer closed the connection.",
        SSL_ERROR_NOT_TRUSTED_ROOT => {
            "The peer's certificate chains to no trusted certificate authority."
        }
        SSL_ERROR_CERT_EXPIRED => "The certificate has expired.",
        SSL_ERROR_BAD_DATE => "The certificate is not yet valid.",
        SSL_ERROR_BAD_KEY_LEN_FOR_EXPORT => "The key length is not allowed for export.",
        SSL_ERROR_NOT_KEYRING => {
            "The file is not a certificate store, or the password is not correct."
        }
        SSL_ERROR_KEYPASSWORD_EXPIRED => "The store password has expired.",
        SSL_ERROR_CERTIFICATE_REJECTED => "The peer rejected the certificate.",
        SSL_ERROR_SSL_NOT_AVAILABLE => "Secure sockets support is not available.",
        SSL_ERROR_NO_INIT => "SSL_Init or SSL_Init_Application has not been called.",
        SSL_ERROR_NO_KEYRING => "The certificate store does not exist.",
        SSL_ERROR_NOT_ENABLED => "Secure sockets are not enabled.",
        SSL_ERROR_BAD_CIPHER_SUITE => "The cipher suite list has no usable entry.",
        SSL_ERROR_CLOSED => "The secure session has been closed.",
        SSL_ERROR_UNKNOWN => "An error with no closer description occurred.",
        SSL_ERROR_NOT_REGISTERED => "The application ID is not registered.",
        SSL_ERROR_NO_CERTIFICATE_AUTHORITIES => "The application trusts no certificate authority.",
        _ => "The return code is not one the secure sockets layer knows.",
    }
}
