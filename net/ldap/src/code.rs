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
}

#[cfg(test)]
mod tests {
    use super::ResultCode;

    #[test]
    fn numbers_round_trip() {
        for n in 0..128 {
            assert_eq!(ResultCode::from_number(n).number(), n);
        }
    }
}
