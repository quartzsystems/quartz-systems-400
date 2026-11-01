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
        "HTP8022",
        0,
        "Configuration of HTTP server instance &1 changed.",
        "The configuration file of instance &1 was replaced. The previous file was kept as httpd.conf.bak in the same directory.",
        "A running instance uses the new configuration after it is restarted.",
    ),
    d(
        "HTP8100",
        30,
        "Configuration error at line &1 column &2: &3",
        "The configuration file &4 could not be used. The directive at line &1, column &2 is not valid: &3",
        "Correct the line on the Configuration file page of the instance and validate the file again. The file in use was not changed.",
    ),
    d(
        "HTP8101",
        0,
        "Configuration file &1 has no errors.",
        "Every line of the configuration file was read and checked. The file was not changed.",
        "None.",
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
        "HTTP server instance &1 cannot be stopped or restarted from its own administration session.",
        "Instance &1 serves this administration session. Stopping or restarting it would end the session in which the request was made.",
        "End or restart the instance from a 5250 session with ENDTCPSVR SERVER(*HTTP) HTTPSVR(&1) and STRTCPSVR SERVER(*HTTP) HTTPSVR(&1).",
    ),
    d(
        "HTP8105",
        30,
        "Port &1 is already used by HTTP server instance &2.",
        "A Listen directive for port &1 already exists in the configuration of instance &2.",
        "Choose another port, or remove the existing Listen entry first.",
    ),
    d(
        "HTP8106",
        30,
        "Files of HTTP server instance &1 could not be used.",
        "Reading or writing a file of instance &1 failed: &2",
        "Check that the server root of the instance exists and that the administration server can write to it, then try again.",
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
        "HTP8120",
        30,
        "Value &1 is not valid for &2.",
        "&2 must be &3.",
        "Correct the value and try again.",
    ),
    d(
        "HTP8122",
        30,
        "HTTP server instance &1 already exists.",
        "An instance named &1 is already defined on this system.",
        "Choose another name, or manage the existing instance from the Server instances page.",
    ),
    d(
        "HTP8123",
        30,
        "No more HTTP server instances can be created.",
        "The system already has &1 instances, the most Global settings allow.",
        "Delete an instance that is no longer needed, or raise the limit in Global settings.",
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
        "Certificate &1 created in store &2 and signed by &3.",
        "A new key pair and a certificate labeled &1, signed by &3, were added to certificate store &2.",
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
    d(
        "HTP8303",
        0,
        "Certificate request &1 created in store &2.",
        "A key pair labeled &1 was added to certificate store &2 with a certificate signing request for it. The entry waits for its certificate.",
        "Download the request and send it to a certificate authority. Import the certificate it returns into store &2 with label &1.",
    ),
    d(
        "HTP8304",
        0,
        "Certificate &1 removed from store &2.",
        "The entry labeled &1 was removed from certificate store &2. Applications that were assigned it no longer have a certificate.",
        "Assign another certificate to those applications on the Applications page.",
    ),
    d(
        "HTP8305",
        0,
        "Certificate &1 is now the default certificate of store &2.",
        "Servers that open certificate store &2 without naming a certificate present &1.",
        "None.",
    ),
    d(
        "HTP8306",
        0,
        "Certificate store &1 created.",
        "An empty certificate store &1 was created at &2. Its password is stashed so that servers can open it.",
        "Create or import certificates into the store from its page.",
    ),
    d(
        "HTP8307",
        0,
        "Password of certificate store &1 changed.",
        "Certificate store &1 was sealed again under the new password and its stashed password was replaced.",
        "None.",
    ),
    d(
        "HTP8308",
        0,
        "Certificate store &1 deleted.",
        "The store file of certificate store &1 and its stashed password were removed.",
        "None.",
    ),
    d(
        "HTP8311",
        0,
        "Certificate assignment removed from application &1.",
        "Application &1 has no certificate assigned. Its server presents the default certificate of the *SYSTEM store, if any, the next time it starts.",
        "Assign a certificate to the application on the Applications page.",
    ),
    d(
        "HTP8320",
        30,
        "Certificate function not done: &1.",
        "The certificate manager refused the request: &1. Nothing was changed.",
        "Correct the request and try again.",
    ),
    d(
        "HTP8321",
        30,
        "Password not correct for certificate store &1.",
        "The password typed does not open certificate store &1. Nothing was changed.",
        "Type the store's current password. If it is lost, delete the store and create it again.",
    ),
    d(
        "HTP8322",
        30,
        "Value not valid: &1.",
        "A value on the page cannot be used: &1.",
        "Correct the value and try again.",
    ),
    d(
        "HTP8330",
        0,
        "Global settings changed.",
        "The settings were written to &1. A new session timeout applies from each session's next request.",
        "None.",
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
    Message::new(kind, id, &text).with_data(data)
}

/// Where a message file's second-level text starts the recovery.
const RECOVERY: &str = "&N Recovery  . . . :   ";
/// Where it starts the cause.
const CAUSE: &str = "&N Cause . . . . . :   ";

/// A description's second-level text as a message file keeps it, with
/// the `&N` line breaks `DSPMSGD` shows.
pub fn help_text(cause: &str, recovery: &str) -> String {
    format!("{CAUSE}{cause} {RECOVERY}{recovery}")
}

/// Cause and recovery from a message file's second-level text; text
/// without the headings is all cause.
pub fn split_help(help: &str) -> (String, String) {
    let h = help.trim();
    let h = h.strip_prefix(CAUSE.trim_end()).map_or(h, str::trim_start);
    match h.split_once(RECOVERY.trim_end()) {
        Some((c, r)) => (c.trim().to_string(), r.trim().to_string()),
        None => (h.to_string(), String::new()),
    }
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
