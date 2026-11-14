// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The numbers of the interface, as the QS/400 headers declare them:
//! result codes, options, scopes, message types, controls. Where the LDAP
//! C API draft (draft-ietf-ldapext-ldap-c-api) or the RFCs give a number,
//! it is theirs; the rest are QS/400's (docs/DIRSRV.md §13). [`TABLE`]
//! is what the headers and the RPG copy member are generated from.

#![allow(missing_docs)]

/// The header a name belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hdr {
    /// `QSYSINC/H(LDAP)`.
    Ldap,
    /// `QSYSINC/H(LBER)`.
    Lber,
    /// `QSYSINC/H(LDAPSSL)`.
    Ssl,
}

/// A constant's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Val {
    /// A number.
    Int(i64),
    /// A string (an OID).
    Str(&'static str),
}

macro_rules! table {
    ($( $hdr:ident { $( $name:ident = $v:expr ),* $(,)? } )*) => {
        $( $( pub const $name: i32 = $v; )* )*
        /// Every numeric constant, by header.
        pub const TABLE: &[(Hdr, &str, Val)] = &[
            $( $( (Hdr::$hdr, stringify!($name), Val::Int($v as i64)), )* )*
        ];
    };
}

table! {
    Ldap {
        LDAP_PORT = 389,
        LDAPS_PORT = 636,
        LDAP_VERSION2 = 2,
        LDAP_VERSION3 = 3,
        LDAP_VERSION_MIN = 2,
        LDAP_VERSION_MAX = 3,
        LDAP_NO_LIMIT = 0,

        LDAP_OPT_API_INFO = 0x00,
        LDAP_OPT_DESC = 0x01,
        LDAP_OPT_DEREF = 0x02,
        LDAP_OPT_SIZELIMIT = 0x03,
        LDAP_OPT_TIMELIMIT = 0x04,
        LDAP_OPT_REFHOPLIMIT = 0x05,
        LDAP_OPT_REBIND_FN = 0x06,
        LDAP_OPT_REBIND_ARG = 0x07,
        LDAP_OPT_REFERRALS = 0x08,
        LDAP_OPT_RESTART = 0x09,
        LDAP_OPT_SSL_CIPHER = 0x0A,
        LDAP_OPT_SSL_TIMEOUT = 0x0B,
        LDAP_OPT_DEBUG = 0x0C,
        LDAP_OPT_UTF8_IO = 0x0E,
        LDAP_OPT_PROTOCOL_VERSION = 0x11,
        LDAP_OPT_SERVER_CONTROLS = 0x12,
        LDAP_OPT_CLIENT_CONTROLS = 0x13,
        LDAP_OPT_HOST_NAME = 0x30,
        LDAP_OPT_ERROR_NUMBER = 0x31,
        LDAP_OPT_ERROR_STRING = 0x32,
        LDAP_OPT_MATCHED_DN = 0x33,
        LDAP_OPT_EXT_ERROR = 0x34,
        LDAP_OPT_ON = 1,
        LDAP_OPT_OFF = 0,

        LDAP_DEREF_NEVER = 0,
        LDAP_DEREF_SEARCHING = 1,
        LDAP_DEREF_FINDING = 2,
        LDAP_DEREF_ALWAYS = 3,

        LDAP_SCOPE_BASE = 0,
        LDAP_SCOPE_ONELEVEL = 1,
        LDAP_SCOPE_SUBTREE = 2,

        LDAP_MOD_ADD = 0,
        LDAP_MOD_DELETE = 1,
        LDAP_MOD_REPLACE = 2,
        LDAP_MOD_INCREMENT = 3,
        LDAP_MOD_BVALUES = 0x80,

        LDAP_AUTH_NONE = 0x00,
        LDAP_AUTH_SIMPLE = 0x80,
        LDAP_AUTH_SASL = 0xA3,

        LDAP_MSG_ONE = 0,
        LDAP_MSG_ALL = 1,
        LDAP_MSG_RECEIVED = 2,
        LDAP_RES_ANY = -1,
        LDAP_RES_BIND = 0x61,
        LDAP_RES_SEARCH_ENTRY = 0x64,
        LDAP_RES_SEARCH_RESULT = 0x65,
        LDAP_RES_MODIFY = 0x67,
        LDAP_RES_ADD = 0x69,
        LDAP_RES_DELETE = 0x6B,
        LDAP_RES_MODRDN = 0x6D,
        LDAP_RES_COMPARE = 0x6F,
        LDAP_RES_SEARCH_REFERENCE = 0x73,
        LDAP_RES_EXTENDED = 0x78,
        LDAP_RES_INTERMEDIATE = 0x79,

        LDAP_SUCCESS = 0x00,
        LDAP_OPERATIONS_ERROR = 0x01,
        LDAP_PROTOCOL_ERROR = 0x02,
        LDAP_TIMELIMIT_EXCEEDED = 0x03,
        LDAP_SIZELIMIT_EXCEEDED = 0x04,
        LDAP_COMPARE_FALSE = 0x05,
        LDAP_COMPARE_TRUE = 0x06,
        LDAP_STRONG_AUTH_NOT_SUPPORTED = 0x07,
        LDAP_STRONG_AUTH_REQUIRED = 0x08,
        LDAP_PARTIAL_RESULTS = 0x09,
        LDAP_REFERRAL = 0x0A,
        LDAP_ADMIN_LIMIT_EXCEEDED = 0x0B,
        LDAP_UNAVAILABLE_CRITICAL_EXTENSION = 0x0C,
        LDAP_CONFIDENTIALITY_REQUIRED = 0x0D,
        LDAP_SASL_BIND_IN_PROGRESS = 0x0E,
        LDAP_NO_SUCH_ATTRIBUTE = 0x10,
        LDAP_UNDEFINED_TYPE = 0x11,
        LDAP_INAPPROPRIATE_MATCHING = 0x12,
        LDAP_CONSTRAINT_VIOLATION = 0x13,
        LDAP_TYPE_OR_VALUE_EXISTS = 0x14,
        LDAP_INVALID_SYNTAX = 0x15,
        LDAP_NO_SUCH_OBJECT = 0x20,
        LDAP_ALIAS_PROBLEM = 0x21,
        LDAP_INVALID_DN_SYNTAX = 0x22,
        LDAP_IS_LEAF = 0x23,
        LDAP_ALIAS_DEREF_PROBLEM = 0x24,
        LDAP_INAPPROPRIATE_AUTH = 0x30,
        LDAP_INVALID_CREDENTIALS = 0x31,
        LDAP_INSUFFICIENT_ACCESS = 0x32,
        LDAP_BUSY = 0x33,
        LDAP_UNAVAILABLE = 0x34,
        LDAP_UNWILLING_TO_PERFORM = 0x35,
        LDAP_LOOP_DETECT = 0x36,
        LDAP_NAMING_VIOLATION = 0x40,
        LDAP_OBJECT_CLASS_VIOLATION = 0x41,
        LDAP_NOT_ALLOWED_ON_NONLEAF = 0x42,
        LDAP_NOT_ALLOWED_ON_RDN = 0x43,
        LDAP_ALREADY_EXISTS = 0x44,
        LDAP_NO_OBJECT_CLASS_MODS = 0x45,
        LDAP_RESULTS_TOO_LARGE = 0x46,
        LDAP_AFFECTS_MULTIPLE_DSAS = 0x47,
        LDAP_OTHER = 0x50,
        LDAP_SERVER_DOWN = 0x51,
        LDAP_LOCAL_ERROR = 0x52,
        LDAP_ENCODING_ERROR = 0x53,
        LDAP_DECODING_ERROR = 0x54,
        LDAP_TIMEOUT = 0x55,
        LDAP_AUTH_UNKNOWN = 0x56,
        LDAP_FILTER_ERROR = 0x57,
        LDAP_USER_CANCELLED = 0x58,
        LDAP_PARAM_ERROR = 0x59,
        LDAP_NO_MEMORY = 0x5A,
        LDAP_CONNECT_ERROR = 0x5B,
        LDAP_NOT_SUPPORTED = 0x5C,
        LDAP_CONTROL_NOT_FOUND = 0x5D,
        LDAP_NO_RESULTS_RETURNED = 0x5E,
        LDAP_MORE_RESULTS_TO_RETURN = 0x5F,
        LDAP_CLIENT_LOOP = 0x60,
        LDAP_REFERRAL_LIMIT_EXCEEDED = 0x61,

        LDAP_CHANGETYPE_ADD = 1,
        LDAP_CHANGETYPE_DELETE = 2,
        LDAP_CHANGETYPE_MODIFY = 4,
        LDAP_CHANGETYPE_MODDN = 8,
        LDAP_CHANGETYPE_ANY = 15,

        LDAP_PP_WARNING_NONE = 0,
        LDAP_PP_TIME_BEFORE_EXPIRATION = 1,
        LDAP_PP_GRACE_LOGINS_REMAINING = 2,
        LDAP_PP_NO_ERROR = -1,
        LDAP_PP_PASSWORD_EXPIRED = 0,
        LDAP_PP_ACCOUNT_LOCKED = 1,
        LDAP_PP_CHANGE_AFTER_RESET = 2,
        LDAP_PP_MOD_NOT_ALLOWED = 3,
        LDAP_PP_MUST_SUPPLY_OLD = 4,
        LDAP_PP_INSUFFICIENT_QUALITY = 5,
        LDAP_PP_TOO_SHORT = 6,
        LDAP_PP_TOO_YOUNG = 7,
        LDAP_PP_IN_HISTORY = 8,
    }
    Ssl {
        LDAP_SSL_ALREADY_INITIALIZED = 0x70,
        LDAP_SSL_INITIALIZE_FAILED = 0x71,
        LDAP_SSL_CLIENT_INIT_NOT_CALLED = 0x72,
        LDAP_SSL_PARAM_ERROR = 0x73,
        LDAP_SSL_HANDSHAKE_FAILED = 0x74,
        LDAP_SSL_GET_CIPHER_FAILED = 0x75,
        LDAP_SSL_NOT_AVAILABLE = 0x76,
    }
    Lber {
        LBER_BOOLEAN = 0x01,
        LBER_INTEGER = 0x02,
        LBER_BITSTRING = 0x03,
        LBER_OCTETSTRING = 0x04,
        LBER_NULL = 0x05,
        LBER_ENUMERATED = 0x0A,
        LBER_SEQUENCE = 0x30,
        LBER_SET = 0x31,
        LBER_USE_DER = 0x01,
    }
}

/// `LBER_ERROR` and `LBER_DEFAULT`: an unsigned tag of all ones.
pub const LBER_ERROR: u32 = 0xFFFF_FFFF;

/// The control and extended operation OIDs the headers name.
pub const OIDS: &[(Hdr, &str, &str)] = &[
    (
        Hdr::Ldap,
        "LDAP_CONTROL_PAGEDRESULTS",
        ldap::control::PAGED_RESULTS,
    ),
    (
        Hdr::Ldap,
        "LDAP_CONTROL_SORTREQUEST",
        ldap::control::SORT_REQUEST,
    ),
    (
        Hdr::Ldap,
        "LDAP_CONTROL_SORTRESPONSE",
        ldap::control::SORT_RESPONSE,
    ),
    (
        Hdr::Ldap,
        "LDAP_CONTROL_MANAGEDSAIT",
        ldap::control::MANAGE_DSA_IT,
    ),
    (
        Hdr::Ldap,
        "LDAP_CONTROL_PERSISTENTSEARCH",
        ldap::control::PERSISTENT_SEARCH,
    ),
    (
        Hdr::Ldap,
        "LDAP_CONTROL_ENTRYCHANGE",
        ldap::control::ENTRY_CHANGE,
    ),
    (
        Hdr::Ldap,
        "LDAP_CONTROL_PROXYAUTH",
        ldap::control::PROXIED_AUTHZ,
    ),
    (
        Hdr::Ldap,
        "LDAP_CONTROL_TREEDELETE",
        ldap::control::TREE_DELETE,
    ),
    (
        Hdr::Ldap,
        "LDAP_CONTROL_ASSERTION",
        ldap::control::ASSERTION,
    ),
    (
        Hdr::Ldap,
        "LDAP_CONTROL_PWDPOLICY",
        ldap::control::PASSWORD_POLICY,
    ),
    (Hdr::Ldap, "LDAP_CONTROL_VLVREQUEST", VLV_REQUEST),
    (Hdr::Ldap, "LDAP_CONTROL_VLVRESPONSE", VLV_RESPONSE),
    (Hdr::Ldap, "LDAP_EXOP_START_TLS", ldap::ext::START_TLS),
    (Hdr::Ldap, "LDAP_EXOP_WHO_AM_I", ldap::ext::WHO_AM_I),
    (
        Hdr::Ldap,
        "LDAP_EXOP_MODIFY_PASSWD",
        ldap::ext::PASSWORD_MODIFY,
    ),
];

/// The virtual list view request control (draft-ietf-ldapext-ldapv3-vlv).
pub const VLV_REQUEST: &str = "2.16.840.1.113730.3.4.9";
/// The virtual list view response control.
pub const VLV_RESPONSE: &str = "2.16.840.1.113730.3.4.10";

/// The text of a result code (`ldap_err2string`), Quartz's.
#[must_use]
pub fn err_text(code: i32) -> &'static str {
    match code {
        LDAP_SSL_ALREADY_INITIALIZED => "The secure connection environment is already set up",
        LDAP_SSL_INITIALIZE_FAILED => "The secure connection environment could not be set up",
        LDAP_SSL_CLIENT_INIT_NOT_CALLED => "No secure connection environment was set up",
        LDAP_SSL_PARAM_ERROR => "A secure connection parameter is not valid",
        LDAP_SSL_HANDSHAKE_FAILED => "The secure connection handshake failed",
        LDAP_SSL_GET_CIPHER_FAILED => "The cipher of the secure connection is not known",
        LDAP_SSL_NOT_AVAILABLE => "Secure connections are not available",
        LDAP_PARTIAL_RESULTS => "Partial results returned",
        LDAP_IS_LEAF => "The entry is a leaf",
        LDAP_RESULTS_TOO_LARGE => "The results are too large",
        LDAP_OTHER => "Unknown error",
        c => match u32::try_from(c) {
            Ok(n) => ldap::code::client_text(n)
                .unwrap_or_else(|| ldap::ResultCode::from_number(n).text()),
            Err(_) => "Unknown error",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_texts_quartz() {
        let mut seen = std::collections::HashSet::new();
        for (_, n, _) in TABLE {
            assert!(seen.insert(*n), "{n} twice");
        }
        for (_, n, _) in OIDS {
            assert!(seen.insert(*n), "{n} twice");
        }
        assert_eq!(err_text(LDAP_NOT_SUPPORTED), "Not supported");
        assert_eq!(err_text(LDAP_SUCCESS), "Success");
        for c in 0..0x80 {
            assert!(!err_text(c).is_empty());
        }
    }
}
