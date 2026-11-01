// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The messages this product owns (message file `QHTTPSVR/QZHBMSG`):
//! first- and second-level text, Quartz-written. The message file is
//! built from this table; the product falls back to it when the system
//! has no text.

use qsweb::msg::{Kind, Message, SecondLevel, substitute};

/// One message description.
#[derive(Clone, Copy, Debug)]
pub struct Desc {
    /// `HTP8001`.
    pub id: &'static str,
    /// Severity, 00 to 99.
    pub severity: u8,
    /// First-level text with `&1`... substitutions.
    pub text: &'static str,
    /// Cause.
    pub cause: &'static str,
    /// Recovery.
    pub recovery: &'static str,
}

const fn d(id: &'static str, severity: u8, text: &'static str, cause: &'static str, recovery: &'static str) -> Desc {
    Desc {
        id,
        severity,
        text,
        cause,
        recovery,
    }
}

/// Every message of `QZHBMSG`.
pub const MESSAGES: &[Desc] = &[
    d(
        "HTP8000",
        0,
        "HTTP server instance &1 starting.",
        "A request to start instance &1 was accepted. The instance's job is being started in subsystem QHTTPSVR.",
        "None. Message HTP8001 follows when the instance is listening.",
    ),
    d(
        "HTP8001",
        0,
        "HTTP server instance &1 started, listening on &2.",
        "Instance &1 read its configuration and is accepting connections on &2.",
        "None.",
    ),
    d(
        "HTP8002",
        0,
        "HTTP server instance &1 ended.",
        "Instance &1 was ended by &2. Its job in subsystem QHTTPSVR is no longer active.",
        "Start the instance again from the Server instances page or with STRTCPSVR SERVER(*HTTP) HTTPSVR(&1).",
    ),
    d(
        "HTP8010",
        0,
        "Certificate &1 loaded for application &2.",
        "The server presents certificate &1 from the *SYSTEM store on its TLS ports.",
        "None.",
    ),
    d(
        "HTP8020",
        0,
        "HTTP server instance &1 created.",
        "The server root &2, its configuration file and the registry entry of instance &1 were created. The instance is stopped.",
        "Start the instance from the Server instances page.",
    ),
    d(
        "HTP8021",
        0,
        "HTTP server instance &1 deleted.",
        "The server root &2 and the registry entry of instance &1 were removed.",
        "None.",
    ),
    d(
        "HTP8100",
        30,
        "Configuration error at line &1 column &2: &3",
        "The configuration file &4 could not be used. The directive at line &1, column &2 is not valid: &3",
        "Correct the line on the Configuration file page of the instance and validate the file again. The file in use was not changed.",
    ),
    d(
        "HTP8103",
        30,
        "HTTP server instance &1 must be stopped first.",
        "Instance &1 is running. The request needs the instance to be stopped.",
        "Stop the instance from the Server instances page, then try the request again.",
    ),
    d(
        "HTP8104",
        30,
        "HTTP server instance &1 cannot be stopped from its own administration session.",
        "Instance &1 serves this administration session. Stopping it would end the session in which the request was made.",
        "Stop the instance from a 5250 session with ENDTCPSVR SERVER(*HTTP) HTTPSVR(&1), or restart it instead.",
    ),
    d(
        "HTP8110",
        30,
        "Request not authorized for user &1.",
        "User &1 does not have the authority the request needs (&2). The attempt was recorded in the audit journal.",
        "Ask the security officer for the authority, or sign on as a user who has it.",
    ),
    d(
        "HTP8111",
        30,
        "User &1 is not an administrator of the HTTP server.",
        "Signing on to HTTP Server Administration needs the *IOSYSCFG special authority or membership in group profile &2.",
        "Ask the security officer for the authority, or sign on as a user who has it.",
    ),
    d(
        "HTP8201",
        30,
        "Server user profile &1 not found or not authorized.",
        "The user profile &1 given for the instance's server jobs does not exist on this system, or it cannot be used by the server.",
        "Create the user profile with CRTUSRPRF, or change the instance to use an existing profile such as QTMHHTTP, then try again.",
    ),
    d(
        "HTP8202",
        30,
        "Port &1 needs *IOSYSCFG for server user profile &2.",
        "Ports below 1024 can only be used by an instance whose server user profile has the *IOSYSCFG special authority. Profile &2 does not.",
        "Choose a port of 1024 or above, or give the server user profile the authority.",
    ),
    d(
        "HTP8301",
        0,
        "Certificate &1 created in store &2.",
        "A key pair and certificate labeled &1 were created in certificate store &2.",
        "Assign the certificate to an application on the Applications page to use it.",
    ),
    d(
        "HTP8302",
        0,
        "Certificate &1 imported into store &2.",
        "The certificate labeled &1 was added to certificate store &2.",
        "Assign the certificate to an application on the Applications page to use it.",
    ),
    d(
        "HTP8310",
        0,
        "Certificate &1 assigned to application &2.",
        "Application &2 presents certificate &1 the next time its server starts.",
        "Restart the server that uses application &2 to present the new certificate now.",
    ),
];

/// The description of `id`.
pub fn desc(id: &str) -> Option<&'static Desc> {
    MESSAGES.iter().find(|m| m.id == id)
}

/// The second-level text of `id` with `data`.
pub fn second_level(id: &str, data: &[&str]) -> Option<SecondLevel> {
    desc(id).map(|m| SecondLevel {
        id: m.id.into(),
        text: substitute(m.text, data),
        cause: substitute(m.cause, data),
        recovery: substitute(m.recovery, data),
        severity: m.severity,
    })
}

/// A message row for `id` with `data`.
pub fn message(kind: Kind, id: &str, data: &[&str]) -> Message {
    let text = desc(id).map_or_else(|| id.to_string(), |m| substitute(m.text, data));
    Message::new(kind, id, &text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_well_formed() {
        for (i, m) in MESSAGES.iter().enumerate() {
            assert!(m.id.starts_with("HTP") && m.id.len() == 7, "{}", m.id);
            assert!(MESSAGES[i + 1..].iter().all(|n| n.id != m.id), "{} twice", m.id);
            assert!(m.text.ends_with('.') || m.text.ends_with("&3"), "{}", m.id);
        }
        assert_eq!(
            message(Kind::Done, "HTP8001", &["API", "*:9443"]).text,
            "HTTP server instance API started, listening on *:9443."
        );
        assert_eq!(second_level("HTP8103", &["WEBSTORE"]).unwrap().severity, 30);
    }
}
