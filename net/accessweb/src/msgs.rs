// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The messages this product owns (message file `QUSRSYS/QZXWMSG`):
//! the `XWA` messages, Quartz-written, and the `QAD` messages it shares
//! with QS/400 Access for Desktop, in the desktop's own words (its
//! `qsa-msg`). The message file is built from this table; the product
//! falls back to it when the system has no text. Every other message a
//! page shows is the command's or API's behind it.

use qsweb::msg::{Kind, Message, SecondLevel, substitute};

/// The message file and its library.
pub const FILE: (&str, &str) = ("QZXWMSG", "QUSRSYS");

/// One message description.
#[derive(Clone, Copy, Debug)]
pub struct Desc {
    /// `XWA0001`.
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

/// A policy denies the function.
pub const DENIED: &str = "XWA0001";
/// Policies changed.
pub const POLICIES_CHANGED: &str = "XWA0002";
/// Preferences changed.
pub const PREFS_CHANGED: &str = "XWA0003";
/// A value is not valid.
pub const BAD_VALUE: &str = "XWA0004";
/// Preferences back to the defaults.
pub const PREFS_RESET: &str = "XWA0005";
/// A function is not available yet.
pub const NOT_AVAILABLE: &str = "XWA0099";
/// A 5250 session saved.
pub const SESSION_SAVED: &str = "XWA0010";
/// A saved 5250 session deleted.
pub const SESSION_DELETED: &str = "XWA0011";
/// A macro saved.
pub const MACRO_SAVED: &str = "XWA0013";
/// A macro deleted.
pub const MACRO_DELETED: &str = "XWA0014";
/// The password has expired (the desktop's).
pub const PASSWORD_EXPIRED: &str = "QAD4022";
/// Password changed (the desktop's).
pub const PASSWORD_CHANGED: &str = "QAD4023";
/// New password and confirmation differ (the desktop's).
pub const PASSWORD_MISMATCH: &str = "QAD4027";
/// The password expires soon (the desktop's).
pub const PASSWORD_EXPIRES: &str = "QAD4031";

/// Every message of `QZXWMSG`.
pub const MESSAGES: &[Desc] = &[
    d(
        DENIED,
        40,
        "Function &1 is not allowed for user profile &2 by the Access for Web policies.",
        "The Access for Web policy for &1 is set to Deny for &2, one of its groups or *PUBLIC. A Deny at any of these levels denies the function, whatever the object authority of the user profile.",
        "Ask your security administrator to change the policy on the Policies page of Access for Web, or use the function from a 5250 display.",
    ),
    d(
        POLICIES_CHANGED,
        0,
        "Access for Web policies for &1 changed.",
        "The policies set for &1 were saved in the policy store. They apply from the next request of every user they affect.",
        "None.",
    ),
    d(
        PREFS_CHANGED,
        0,
        "Access for Web preferences for user profile &1 changed.",
        "The preferences of &1 were saved. They apply from the next page.",
        "None.",
    ),
    d(
        BAD_VALUE,
        30,
        "Value &1 is not valid for &2.",
        "The value &1 typed for &2 is not one this field accepts. Nothing was saved.",
        "Type a value the field accepts and try again.",
    ),
    d(
        PREFS_RESET,
        0,
        "Access for Web preferences for user profile &1 set to the defaults.",
        "Every preference of &1 was set back to the value the product ships with.",
        "None.",
    ),
    d(
        SESSION_SAVED,
        0,
        "Session &1 saved.",
        "The settings of 5250 session &1 were saved with your Access for Web sessions.",
        "None.",
    ),
    d(
        SESSION_DELETED,
        0,
        "Session &1 deleted.",
        "The saved 5250 session &1, its keyboard map and its macros were removed from your Access for Web sessions.",
        "None.",
    ),
    d(
        MACRO_SAVED,
        0,
        "Macro &1 saved.",
        "The steps of macro &1 were saved with its 5250 session.",
        "None.",
    ),
    d(
        MACRO_DELETED,
        0,
        "Macro &1 deleted.",
        "Macro &1 was removed from its 5250 session.",
        "None.",
    ),
    d(
        "XWA0020",
        0,
        "Request &1 saved.",
        "Request &1 was saved with your Access for Web requests.",
        "None.",
    ),
    d(
        "XWA0030",
        0,
        "Copy completed: &1 rows copied, &2 errors.",
        "The copy wrote &1 rows. &2 rows could not be written; the messages of the copy name them.",
        "None when there were no errors. Otherwise correct the rows named and copy them again.",
    ),
    d(
        "XWA0040",
        0,
        "File &1 uploaded.",
        "The file &1 was written to the integrated file system as your user profile.",
        "None.",
    ),
    d(
        "XWA0050",
        0,
        "Command submitted to job queue &1 as job &2.",
        "The command runs in batch job &2 from job queue &1, as your user profile.",
        "Look at the job from My jobs; its job log holds the command's messages.",
    ),
    d(
        NOT_AVAILABLE,
        40,
        "Function &1 of Access for Web is not available on this system.",
        "The pages of &1 are not part of this level of the product.",
        "Use the function from a 5250 display or from QS/400 Access for Desktop.",
    ),
    d(
        PASSWORD_EXPIRED,
        20,
        "Password for &1 on &2 has expired.",
        "The password of user &1 on system &2 is correct but has expired. The system requires a new one before it allows a sign-on.",
        "Change the password in the Change Password dialog. The new password must meet the system's password rules, which the system reports if it is refused.",
    ),
    d(
        PASSWORD_CHANGED,
        0,
        "Password changed for &1 on &2.",
        "System &2 accepted the new password for user &1.",
        "None needed. Use the new password from now on, including at the 5250 sign-on display.",
    ),
    d(
        PASSWORD_MISMATCH,
        30,
        "New password and confirmation are not the same.",
        "The new password and the confirmation of it were typed differently. Nothing was sent to the system.",
        "Type the new password again in both fields.",
    ),
    d(
        PASSWORD_EXPIRES,
        10,
        "Password for &1 on &2 expires in &3 days.",
        "The password of user profile &1 on system &2 expires in &3 days, within the system's password expiration warning period (system value QPWDEXPWRN). It is counted on the system's clock from the password's last change and its expiration interval. This warning is shown once per session.",
        "Select Change Password... to change it now, or change it later with Actions > Change Password... before it expires. Once it has expired, sign-on asks for a new one.",
    ),
    d(
        "QAD1002",
        30,
        "Connection to &1 refused.",
        "System &1 answered, but nothing accepted the connection on the port used. The Telnet server may not be started, or the port in the system configuration may be wrong, or TLS may be selected for a system with no TLS listener.",
        "Check that the Telnet server is started on &1, and check the Telnet ports on the Ports tab of the system's properties and the TLS setting on its Security tab. Then select Reconnect.",
    ),
    d(
        "QAD1003",
        30,
        "Host &1 not found.",
        "The name &1 could not be turned into an address. The name may be misspelt, or the name server may not know it.",
        "Correct the system name in System Configurations, or give a fixed IP address on the Connection tab. Then start the session again.",
    ),
    d(
        "QAD1004",
        30,
        "Connection to &1 timed out.",
        "No answer came from system &1 within 15 seconds. The system may be down, or a firewall may be dropping the connection.",
        "Check that &1 is running and can be reached from this computer, then select Reconnect.",
    ),
    d(
        "QAD1005",
        20,
        "Session to &1 ended by the system.",
        "System &1 closed the connection. This happens when the job ends after sign-off, when the device is varied off, or when the connection is lost.",
        "Select Reconnect to start a new session. The last screen stays visible until then.",
    ),
    d(
        "QAD1006",
        0,
        "Session to &1 disconnected.",
        "The session to system &1 was disconnected at your request. What the job does now depends on the device recovery action of the system: it may end, or wait for you to sign on again.",
        "Select Reconnect to start a new session. The last screen stays visible until then.",
    ),
    d(
        "QAD1010",
        30,
        "TLS handshake with &1 failed: &2.",
        "A TLS session could not be set up with system &1. The reason given was: &2",
        "Check that the port on the Ports tab is the system's TLS Telnet port and that the system's TLS configuration is complete. If the system offers no TLS, clear TLS on the Security tab.",
    ),
    d(
        "QAD1011",
        30,
        "Certificate for &1 is not trusted.",
        "System &1 presented a certificate that does not chain to a certificate authority this computer trusts, or that is not valid for the name &1. The reason given was: &2",
        "Compare the certificate's SHA-256 fingerprint with the one shown on the system itself. If they match and you trust the system, select Trust for this system; the certificate is added to this system's CA list. Otherwise do not connect.",
    ),
    d(
        "QAD1020",
        30,
        "Device name &1 is in use.",
        "The system could not give this session the display device &1: the device is in use by another session or is not available (start-up response code &2).",
        "Select Retry without device name to let the system choose a device, or change the device name and start the session again.",
    ),
    d(
        "QAD1021",
        30,
        "Device name &1 is not valid.",
        "The system refused display device name &1 for this session (start-up response code &2: &3).",
        "Select Retry without device name to let the system choose a device, or give a device name of 1 to 10 letters, digits, #, $ or @, starting with a letter.",
    ),
    d(
        "QAD2001",
        30,
        "Session file &1 could not be read: &2.",
        "The file &1 could not be opened, or it is not a valid session, keyboard map or macro file. The reason given was: &2",
        "Correct the file, or open another one. These files are TOML text whose first table is [qsaccess], giving the kind of file and its format version.",
    ),
    d(
        "QAD2002",
        30,
        "Session file &1 was made by a newer version (&2).",
        "The file &1 is format version &2, which is newer than this release of QS/400 Access for Desktop can read. The file was not opened and has not been changed.",
        "Open the file with the release that made it, or install a newer release of QS/400 Access for Desktop.",
    ),
    d(
        "QAD2010",
        0,
        "Keyboard map loaded from &1.",
        "The keyboard map in the file &1 was copied. Later changes to the file do not affect the copy.",
        "None needed. Save the session to keep the map with it.",
    ),
    d(
        "QAD2020",
        30,
        "Macro &1 stopped at step &2: &3.",
        "Macro &1 could not carry out step &2 and was stopped. The reason was: &3. The steps after it were not run.",
        "Check that the session shows the screen the macro expects at step &2, then run the macro again. To change the step, open the macro in the Macro editor.",
    ),
    d(
        "QAD2021",
        0,
        "Macro &1 recorded with &2 steps.",
        "Recording ended and macro &1 was saved with &2 steps. Text typed into fields that are not displayed, such as passwords, was not recorded; a step that asks for the value when the macro runs takes its place.",
        "None needed. Run the macro from the session's Macro menu, or open it in the Macro editor to change it.",
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

/// A description's second-level text as a message file keeps it.
pub fn help_text(cause: &str, recovery: &str) -> String {
    format!("{CAUSE}{cause} {RECOVERY}{recovery}")
}

/// Cause and recovery from a message file's second-level text.
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
            assert!((m.id.starts_with("XWA") || m.id.starts_with("QAD")) && m.id.len() == 7, "{}", m.id);
            assert!(MESSAGES[i + 1..].iter().all(|n| n.id != m.id), "{} twice", m.id);
            assert!(m.text.ends_with('.'), "{}", m.id);
        }
        assert_eq!(
            message(Kind::Info, PASSWORD_EXPIRES, &["CWELLMAN", "QSHQ400", "5"]).text,
            "Password for CWELLMAN on QSHQ400 expires in 5 days."
        );
        let h = help_text("c", "r");
        assert_eq!(split_help(&h), ("c".into(), "r".into()));
    }
}
