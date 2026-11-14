// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Result code names (RFC 4511 appendix A, RFC 4528, RFC 4370, RFC 3909).

/// The name of result code `rc`, or `"?"`.
pub fn name(rc: u32) -> &'static str {
    match rc {
        0 => "success",
        1 => "operationsError",
        2 => "protocolError",
        3 => "timeLimitExceeded",
        4 => "sizeLimitExceeded",
        5 => "compareFalse",
        6 => "compareTrue",
        7 => "authMethodNotSupported",
        8 => "strongerAuthRequired",
        10 => "referral",
        11 => "adminLimitExceeded",
        12 => "unavailableCriticalExtension",
        13 => "confidentialityRequired",
        14 => "saslBindInProgress",
        16 => "noSuchAttribute",
        17 => "undefinedAttributeType",
        18 => "inappropriateMatching",
        19 => "constraintViolation",
        20 => "attributeOrValueExists",
        21 => "invalidAttributeSyntax",
        32 => "noSuchObject",
        33 => "aliasProblem",
        34 => "invalidDNSyntax",
        36 => "aliasDereferencingProblem",
        48 => "inappropriateAuthentication",
        49 => "invalidCredentials",
        50 => "insufficientAccessRights",
        51 => "busy",
        52 => "unavailable",
        53 => "unwillingToPerform",
        54 => "loopDetect",
        64 => "namingViolation",
        65 => "objectClassViolation",
        66 => "notAllowedOnNonLeaf",
        67 => "notAllowedOnRDN",
        68 => "entryAlreadyExists",
        69 => "objectClassModsProhibited",
        71 => "affectsMultipleDSAs",
        80 => "other",
        118 => "canceled",
        122 => "assertionFailed",
        123 => "authorizationDenied",
        _ => "?",
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn names() {
        assert_eq!(super::name(32), "noSuchObject");
        assert_eq!(super::name(122), "assertionFailed");
        assert_eq!(super::name(999), "?");
    }
}
