// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system message texts the sample system's pages show, as the
//! system's message files give them (the product's own come from
//! [`crate::msgs`]). Each area adds the messages it uses.

use qsweb::msg::{SecondLevel, substitute};

/// One message: ID, severity, text, cause, recovery.
type Desc = (&'static str, u8, &'static str, &'static str, &'static str);

/// Every system message the fixture knows.
const MESSAGES: &[Desc] = &[
    (
        "CPF2204",
        40,
        "User profile &1 not found.",
        "No user profile has the name given.",
        "Check the spelling of the user profile name and try again.",
    ),
    (
        "CPF2207",
        40,
        "Not authorized to use object &1 in library &2 type &3.",
        "Your user profile lacks the authority to the object that the request needs.",
        "Ask the security officer or the object's owner for the authority, or use a profile that has it.",
    ),
    (
        "CPF22E2",
        40,
        "The password is not correct for user profile &1.",
        "The password typed is not the profile's password. Each wrong attempt counts toward the limit of system value QMAXSIGN.",
        "Type the correct password and try again.",
    ),
    (
        "CPF22E3",
        40,
        "User profile &1 is disabled.",
        "The profile's status is *DISABLED, so it cannot sign on.",
        "Ask the security officer to enable the profile.",
    ),
    (
        "CPF2234",
        40,
        "The new password does not follow the password rules.",
        "The new password breaks one of the password rules the system values QPWDMINLEN through QPWDRULES set.",
        "Choose a password that follows the rules and try again.",
    ),
];

/// The text of `id` with `data` substituted.
pub fn second_level(id: &str, data: &[&str]) -> Option<SecondLevel> {
    MESSAGES
        .iter()
        .find(|m| m.0 == id)
        .map(|m| SecondLevel {
            id: m.0.into(),
            text: substitute(m.2, data),
            cause: substitute(m.3, data),
            recovery: substitute(m.4, data),
            severity: m.1,
        })
        .or_else(|| crate::msgs::second_level(id, data))
}
