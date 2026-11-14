// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Result codes (RFC 4511 Appendix A).

/// An LDAP result code. Values outside the named ones are kept as they
/// came ([`ResultCode::Other`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum ResultCode {
    Success,
    OperationsError,
    ProtocolError,
    TimeLimitExceeded,
    SizeLimitExceeded,
    CompareFalse,
    CompareTrue,
    AuthMethodNotSupported,
    StrongerAuthRequired,
    Referral,
    AdminLimitExceeded,
    UnavailableCriticalExtension,
    ConfidentialityRequired,
    SaslBindInProgress,
    NoSuchAttribute,
    UndefinedAttributeType,
    InappropriateMatching,
    ConstraintViolation,
    AttributeOrValueExists,
    InvalidAttributeSyntax,
    NoSuchObject,
    AliasProblem,
    InvalidDnSyntax,
    AliasDereferencingProblem,
    InappropriateAuthentication,
    InvalidCredentials,
    InsufficientAccessRights,
    Busy,
    Unavailable,
    UnwillingToPerform,
    LoopDetect,
    NamingViolation,
    ObjectClassViolation,
    NotAllowedOnNonLeaf,
    NotAllowedOnRdn,
    EntryAlreadyExists,
    ObjectClassModsProhibited,
    AffectsMultipleDsas,
    Other(u32),
}

impl ResultCode {
    /// The code's number.
    #[must_use]
    pub const fn number(self) -> u32 {
        match self {
            Self::Success => 0,
            Self::OperationsError => 1,
            Self::ProtocolError => 2,
            Self::TimeLimitExceeded => 3,
            Self::SizeLimitExceeded => 4,
            Self::CompareFalse => 5,
            Self::CompareTrue => 6,
            Self::AuthMethodNotSupported => 7,
            Self::StrongerAuthRequired => 8,
            Self::Referral => 10,
            Self::AdminLimitExceeded => 11,
            Self::UnavailableCriticalExtension => 12,
            Self::ConfidentialityRequired => 13,
            Self::SaslBindInProgress => 14,
            Self::NoSuchAttribute => 16,
            Self::UndefinedAttributeType => 17,
            Self::InappropriateMatching => 18,
            Self::ConstraintViolation => 19,
            Self::AttributeOrValueExists => 20,
            Self::InvalidAttributeSyntax => 21,
            Self::NoSuchObject => 32,
            Self::AliasProblem => 33,
            Self::InvalidDnSyntax => 34,
            Self::AliasDereferencingProblem => 36,
            Self::InappropriateAuthentication => 48,
            Self::InvalidCredentials => 49,
            Self::InsufficientAccessRights => 50,
            Self::Busy => 51,
            Self::Unavailable => 52,
            Self::UnwillingToPerform => 53,
            Self::LoopDetect => 54,
            Self::NamingViolation => 64,
            Self::ObjectClassViolation => 65,
            Self::NotAllowedOnNonLeaf => 66,
            Self::NotAllowedOnRdn => 67,
            Self::EntryAlreadyExists => 68,
            Self::ObjectClassModsProhibited => 69,
            Self::AffectsMultipleDsas => 71,
            Self::Other(n) => n,
        }
    }

    /// The code for a number.
    #[must_use]
    pub const fn from_number(n: u32) -> Self {
        match n {
            0 => Self::Success,
            1 => Self::OperationsError,
            2 => Self::ProtocolError,
            3 => Self::TimeLimitExceeded,
            4 => Self::SizeLimitExceeded,
            5 => Self::CompareFalse,
            6 => Self::CompareTrue,
            7 => Self::AuthMethodNotSupported,
            8 => Self::StrongerAuthRequired,
            10 => Self::Referral,
            11 => Self::AdminLimitExceeded,
            12 => Self::UnavailableCriticalExtension,
            13 => Self::ConfidentialityRequired,
            14 => Self::SaslBindInProgress,
            16 => Self::NoSuchAttribute,
            17 => Self::UndefinedAttributeType,
            18 => Self::InappropriateMatching,
            19 => Self::ConstraintViolation,
            20 => Self::AttributeOrValueExists,
            21 => Self::InvalidAttributeSyntax,
            32 => Self::NoSuchObject,
            33 => Self::AliasProblem,
            34 => Self::InvalidDnSyntax,
            36 => Self::AliasDereferencingProblem,
            48 => Self::InappropriateAuthentication,
            49 => Self::InvalidCredentials,
            50 => Self::InsufficientAccessRights,
            51 => Self::Busy,
            52 => Self::Unavailable,
            53 => Self::UnwillingToPerform,
            54 => Self::LoopDetect,
            64 => Self::NamingViolation,
            65 => Self::ObjectClassViolation,
            66 => Self::NotAllowedOnNonLeaf,
            67 => Self::NotAllowedOnRdn,
            68 => Self::EntryAlreadyExists,
            69 => Self::ObjectClassModsProhibited,
            71 => Self::AffectsMultipleDsas,
            n => Self::Other(n),
        }
    }

    /// Quartz's short text for the code, as `ldap_err2string` returns.
    /// Codes without a name share one text; the client-side codes are
    /// [`client_text`]'s.
    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            Self::Success => "Success",
            Self::OperationsError => "Operations error",
            Self::ProtocolError => "Protocol error",
            Self::TimeLimitExceeded => "Time limit exceeded",
            Self::SizeLimitExceeded => "Size limit exceeded",
            Self::CompareFalse => "Compare false",
            Self::CompareTrue => "Compare true",
            Self::AuthMethodNotSupported => "Authentication method not supported",
            Self::StrongerAuthRequired => "Stronger authentication required",
            Self::Referral => "Referral",
            Self::AdminLimitExceeded => "Administrative limit exceeded",
            Self::UnavailableCriticalExtension => "Critical extension is unavailable",
            Self::ConfidentialityRequired => "Confidentiality required",
            Self::SaslBindInProgress => "SASL bind in progress",
            Self::NoSuchAttribute => "No such attribute",
            Self::UndefinedAttributeType => "Undefined attribute type",
            Self::InappropriateMatching => "Inappropriate matching",
            Self::ConstraintViolation => "Constraint violation",
            Self::AttributeOrValueExists => "Type or value exists",
            Self::InvalidAttributeSyntax => "Invalid syntax",
            Self::NoSuchObject => "No such object",
            Self::AliasProblem => "Alias problem",
            Self::InvalidDnSyntax => "Invalid DN syntax",
            Self::AliasDereferencingProblem => "Alias dereferencing problem",
            Self::InappropriateAuthentication => "Inappropriate authentication",
            Self::InvalidCredentials => "Invalid credentials",
            Self::InsufficientAccessRights => "Insufficient access",
            Self::Busy => "Server is busy",
            Self::Unavailable => "Server is unavailable",
            Self::UnwillingToPerform => "Server is unwilling to perform",
            Self::LoopDetect => "Loop detected",
            Self::NamingViolation => "Naming violation",
            Self::ObjectClassViolation => "Object class violation",
            Self::NotAllowedOnNonLeaf => "Operation not allowed on a non-leaf entry",
            Self::NotAllowedOnRdn => "Operation not allowed on the RDN",
            Self::EntryAlreadyExists => "Entry already exists",
            Self::ObjectClassModsProhibited => "Object class modifications prohibited",
            Self::AffectsMultipleDsas => "Operation affects multiple servers",
            Self::Other(n) => match n {
                118 => "Operation canceled",
                119 => "No such operation",
                120 => "Too late to cancel",
                121 => "Cannot cancel",
                122 => "Assertion failed",
                123 => "Authorization denied",
                _ => match client_text(n) {
                    Some(t) => t,
                    None => "Unknown result code",
                },
            },
        }
    }
}

/// The text of a client-side code (the C API's `LDAP_SERVER_DOWN` and
/// its neighbours, 0x51 to 0x61), none for others.
#[must_use]
pub const fn client_text(n: u32) -> Option<&'static str> {
    Some(match n {
        0x51 => "Cannot reach the server",
        0x52 => "Local error",
        0x53 => "Cannot encode the request",
        0x54 => "Cannot decode the response",
        0x55 => "Timed out",
        0x56 => "Unknown authentication method",
        0x57 => "Bad search filter",
        0x58 => "Canceled by the user",
        0x59 => "Bad parameter",
        0x5A => "Out of memory",
        0x5B => "Cannot connect",
        0x5C => "Not supported",
        0x5D => "Control not found",
        0x5E => "No results returned",
        0x5F => "More results to return",
        0x60 => "Client loop detected",
        0x61 => "Referral hop limit exceeded",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::ResultCode;

    #[test]
    fn texts() {
        assert_eq!(ResultCode::Success.text(), "Success");
        assert_eq!(ResultCode::from_number(0x5C).text(), "Not supported");
        assert_eq!(ResultCode::Other(9).text(), "Unknown result code");
        for n in 0..128 {
            assert!(!ResultCode::from_number(n).text().is_empty());
        }
    }

    #[test]
    fn numbers_round_trip() {
        for n in 0..128 {
            assert_eq!(ResultCode::from_number(n).number(), n);
        }
    }
}
