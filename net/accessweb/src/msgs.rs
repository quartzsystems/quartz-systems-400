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
/// A request saved.
pub const REQUEST_SAVED: &str = "XWA0020";
/// Run selected without a selection.
pub const SELECT_STATEMENTS: &str = "XWA0021";
/// A script without statements.
pub const NO_STATEMENT: &str = "XWA0022";
/// A request deleted.
pub const REQUEST_DELETED: &str = "XWA0023";
/// A request shared.
pub const REQUEST_SHARED: &str = "XWA0024";
/// A file that could not be read.
pub const FILE_UNREADABLE: &str = "XWA0025";
/// A copy request the page cannot take.
pub const NOT_A_REQUEST: &str = "XWA0026";
/// A result without rows to download.
pub const NO_RESULT: &str = "XWA0027";
/// A copy completed.
pub const COPY_DONE: &str = "XWA0030";
/// No more connections (the desktop's).
pub const NO_CONNECTIONS: &str = "QAD7003";
/// The connection was lost (the desktop's).
pub const CONNECTION_LOST: &str = "QAD7010";
/// A result cut short (the desktop's).
pub const INCOMPLETE: &str = "QAD7011";
/// Rows changed (the desktop's).
pub const ROWS_CHANGED: &str = "QAD7020";
/// Uncommitted changes (the desktop's).
pub const UNCOMMITTED: &str = "QAD7021";
/// A transfer complete (the desktop's).
pub const TRANSFER_DONE: &str = "QAD7030";
/// A transfer failed (the desktop's).
pub const TRANSFER_FAILED: &str = "QAD7031";
/// A transfer cancelled (the desktop's).
pub const TRANSFER_CANCELLED: &str = "QAD7032";
/// A preview value that does not fit (the desktop's).
pub const MISFIT: &str = "QAD7035";
/// A request file written (the desktop's).
pub const REQUEST_WRITTEN: &str = "QAD7040";
// Files and Print (steps 13-15).
/// File uploaded.
pub const UPLOADED: &str = "XWA0040";
/// Nothing selected.
pub const NONE_SELECTED: &str = "XWA0041";
/// A file saved from the editor.
pub const SAVED: &str = "XWA0042";
/// An entry's authority changed.
pub const AUTH_CHANGED: &str = "XWA0043";
/// One spooled file is needed.
pub const ONE_SPLF: &str = "XWA0045";
/// Spooled files listed (the desktop's).
pub const SPLF_COUNT: &str = "QAD6001";
/// Spooled file held (the desktop's).
pub const SPLF_HELD: &str = "QAD6002";
/// Spooled file released (the desktop's).
pub const SPLF_RELEASED: &str = "QAD6003";
/// Spooled file deleted (the desktop's).
pub const SPLF_DELETED: &str = "QAD6004";
/// Spooled file moved (the desktop's).
pub const SPLF_MOVED: &str = "QAD6005";
/// Some spooled file actions failed (the desktop's).
pub const SPLF_SUMMARY: &str = "QAD6006";
/// Spooled file still open (the desktop's).
pub const SPLF_OPEN: &str = "QAD6012";
/// Spooled file cannot be shown so (the desktop's).
pub const SPLF_FORMAT: &str = "QAD6013";
/// Folder created (the desktop's).
pub const FOLDER_CREATED: &str = "QAD8002";
/// Renamed or moved (the desktop's).
pub const RENAMED: &str = "QAD8003";
/// Deleted (the desktop's).
pub const DELETED: &str = "QAD8004";
/// Copied (the desktop's).
pub const COPIED: &str = "QAD8005";
/// A delete stopped (the desktop's).
pub const DELETE_STOPPED: &str = "QAD8006";
/// The target exists (the desktop's).
pub const FILE_EXISTS: &str = "QAD8010";
/// Changed since opened (the desktop's).
pub const CHANGED_SINCE: &str = "QAD8011";
/// Too large to edit (the desktop's).
pub const TOO_LARGE: &str = "QAD8012";
/// Not text, opened read-only (the desktop's).
pub const READ_ONLY: &str = "QAD8013";
/// Transfer complete (the desktop's).
pub const FILE_TRANSFER_DONE: &str = "QAD8020";
/// Transfer failed (the desktop's).
pub const FILE_TRANSFER_FAILED: &str = "QAD8021";
/// Transfer cancelled (the desktop's).
pub const FILE_TRANSFER_CANCELLED: &str = "QAD8022";

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
        REQUEST_SAVED,
        0,
        "Request &1 saved.",
        "Request &1 was saved with your Access for Web requests.",
        "None.",
    ),
    d(
        SELECT_STATEMENTS,
        30,
        "Select the statements to run.",
        "Run selected runs the statements of the part of the script that is selected, and the page sent no selection. A browser running the page without script cannot select part of the script.",
        "Use Run statement or Run all, or leave only the statements to run in the script.",
    ),
    d(
        NO_STATEMENT,
        30,
        "There is no statement to run.",
        "The part of the script to run holds no SQL statement: it is empty, or only blanks and comments.",
        "Type a statement, or open a .sql file or a saved request, and run it again.",
    ),
    d(
        REQUEST_DELETED,
        0,
        "Request &1 deleted.",
        "Request &1 was removed from your Access for Web requests. Users it was shared with no longer see it.",
        "None.",
    ),
    d(
        REQUEST_SHARED,
        0,
        "Request &1 shared with &2.",
        "Request &1 is listed for &2 under My requests, to run or copy. Only you can change or delete it.",
        "None. Share it with nobody to keep it to yourself again.",
    ),
    d(
        FILE_UNREADABLE,
        30,
        "File &1 could not be read: &2.",
        "The file &1 sent with the page is not a file of the format chosen, or it is damaged: &2. Nothing was changed on the system.",
        "Check the file and the format and code page chosen, then send the file again.",
    ),
    d(
        NOT_A_REQUEST,
        30,
        "File &1 is not a request this page can open: &2.",
        "The file &1 is not a .qdx or .sql file this page takes: &2.",
        "Open a table transfer request from QS/400 Access for Desktop or Access for Web, or a .sql script.",
    ),
    d(
        NO_RESULT,
        30,
        "Result &1 has no rows to download.",
        "Download as writes the rows of the result shown, and result &1 is not a query's.",
        "Choose the tab of a query's result, or run a query, then download it.",
    ),
    d(
        COPY_DONE,
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
        NO_CONNECTIONS,
        30,
        "No more connections allowed to &1. Close a window and try again.",
        "A session on system &1 may have at most 16 connections, the shared one included, and it has them all: the system refused another (TCP1A59). Each Run SQL and Run Command window and each Data Transfer run has one.",
        "Close a Run SQL, Run Command or Data Transfer window, or wait for a transfer to end, then try again.",
    ),
    d(
        CONNECTION_LOST,
        30,
        "The connection to &1 was lost; the statement must be run again.",
        "The window's connection to system &1 closed while a statement ran or before its result was read. Its job ended with it, and with the job its statements, cursors and uncommitted changes, which the system rolled back. The statement was not run again by this program.",
        "Check what the statement did, if it changes data, and run it again. The next run attaches a new connection; the rows already shown stay, marked incomplete.",
    ),
    d(
        INCOMPLETE,
        10,
        "Result is incomplete: &1.",
        "Not every row of the result was read: &1. The rows shown are those the system sent before that.",
        "Run the statement again for the whole result. The rows shown can still be copied or exported, marked incomplete.",
    ),
    d(
        ROWS_CHANGED,
        0,
        "&1 rows changed.",
        "The statement changed &1 rows. Under commitment control the changes are not committed until Commit.",
        "None needed. Under a commit level other than None, select Commit to keep the changes or Rollback to undo them.",
    ),
    d(
        UNCOMMITTED,
        10,
        "Uncommitted changes in &1. Commit, roll back, or cancel.",
        "Statements in &1 changed data under commitment control, and the changes are neither committed nor rolled back. Closing the connection, or changing the commit level or naming, needs them settled first: a connection that closes rolls them back.",
        "Select Commit to keep the changes, Roll Back to undo them, or Cancel to go back to the window.",
    ),
    d(
        TRANSFER_DONE,
        0,
        "Transfer &1 complete: &2 rows in &3.",
        "Every row of transfer &1 was transferred: &2 rows, in &3.",
        "None needed.",
    ),
    d(
        TRANSFER_FAILED,
        30,
        "Transfer &1 failed at row &2 (line &3 of &4): &5.",
        "The system refused row &2 of transfer &1, from line &3 of &4, with: &5. What was kept depends on the transfer's commitment options; the report says how many rows were committed.",
        "Correct the row in &4, or the column mapping, and run the transfer again; when rows were committed, resume from the row the report gives.",
    ),
    d(
        TRANSFER_CANCELLED,
        10,
        "Transfer &1 was cancelled; nothing was kept.",
        "Transfer &1 was cancelled before it ended. A download's partial output file was deleted; an upload's uncommitted rows were rolled back.",
        "Run the transfer again when you are ready.",
    ),
    d(
        MISFIT,
        20,
        "Column &1: value &2 does not fit &3.",
        "In the preview, value &2 for column &1 cannot be stored in its type &3: it is too long, has too many digits, or is not of that type. Only the first such value of each column is shown.",
        "Correct the value in the source file, or map the column to another target column, before running the transfer.",
    ),
    d(
        REQUEST_WRITTEN,
        0,
        "Request saved to &1.",
        "The transfer request was saved to &1. It holds everything needed to run the transfer again except passwords, which are never saved.",
        "None needed. Open &1, or run it with qsaccess run, to transfer again.",
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
    // Files and Print (steps 13-15).
    d(
        NONE_SELECTED,
        10,
        "No entry was selected.",
        "The action applies to the entries selected in the list, and none was selected. Nothing was done.",
        "Select one or more entries with their check boxes, then select the action again.",
    ),
    d(
        SAVED,
        0,
        "File &1 saved.",
        "The text was written to file &1 in its own CCSID and line endings, as your user profile.",
        "None.",
    ),
    d(
        AUTH_CHANGED,
        0,
        "Authority of &1 changed.",
        "The data authority, the public authority or the authorization list of &1 was changed as your user profile.",
        "None.",
    ),
    d(
        ONE_SPLF,
        10,
        "Select one spooled file for &1.",
        "&1 makes one file from one spooled file, and the selection did not hold exactly one.",
        "Select one spooled file and select &1 again, or use its PDF and Text links in the list.",
    ),
    d(
        "QAD6001",
        0,
        "&1 spooled files on &2.",
        "The Printer Output list shows the spooled files on system &2 that match the filter: &1 in all, a page at a time. Clicking a column heading sorts the page shown, not the whole list.",
        "None needed. Change the filter and select Go to list other files. Select Refresh, or set Auto-refresh, to see files that arrived since.",
    ),
    d(
        "QAD6002",
        0,
        "Spooled file &1 held.",
        "Spooled file &1 was held on the system: it is not printed until it is released. A file being printed when it was held stops immediately or at the end of the current page, as chosen.",
        "To print it, select it and select Release.",
    ),
    d(
        "QAD6003",
        0,
        "Spooled file &1 released.",
        "Spooled file &1 was released: it prints when a writer for its output queue reaches it.",
        "None needed. If it does not print, check in Output Queues that a writer is attached to its queue and that the queue is not held.",
    ),
    d(
        "QAD6004",
        0,
        "Spooled file &1 deleted.",
        "Spooled file &1 was deleted from the system, with its data.",
        "None needed. A deleted spooled file cannot be brought back; run the job that made it again to make it again.",
    ),
    d(
        "QAD6005",
        0,
        "Spooled file &1 moved to &2.",
        "Spooled file &1 is now on output queue &2, at the end of the queue or at its top, as chosen. It prints when a writer for that queue reaches it.",
        "None needed.",
    ),
    d(
        "QAD6006",
        30,
        "&1 of &2 actions failed; see the details.",
        "An action was asked for on &2 spooled files, one request each, and the system refused &1 of them. The summary lists every file, and for each refusal the system's message as it sent it.",
        "Select a refusal's message ID for its second-level text, correct what it names, refresh the list, then try the action again on those files.",
    ),
    d(
        "QAD6012",
        20,
        "Spooled file &1 is still being written.",
        "Spooled file &1 is still open: the program making it has not closed it, so its pages cannot be read yet. The system sent no message of its own for this.",
        "Wait for the job that makes the file to close it, select Refresh, and open the file again.",
    ),
    d(
        "QAD6013",
        20,
        "Spooled file &1 cannot be shown as &2.",
        "The system cannot make &2 of spooled file &1 from the kind of data the file holds, and sent no message of its own saying why.",
        "Look at the file from a 5250 display session (WRKSPLF, option 5), or print it on a printer device that takes its kind of data.",
    ),
    d(
        "QAD8002",
        0,
        "Folder &1 created.",
        "The folder &1 was created on the system, with the authorities its parent folder gives new objects.",
        "None needed.",
    ),
    d(
        "QAD8003",
        0,
        "&1 renamed to &2.",
        "The system renamed or moved &1 to &2.",
        "None needed. To undo it, rename &2 back.",
    ),
    d(
        "QAD8004",
        0,
        "&1 deleted.",
        "&1 was deleted on the system. A deleted file cannot be recovered from this program.",
        "None needed.",
    ),
    d(
        "QAD8005",
        0,
        "&1 copied to &2.",
        "The system copied &1 to &2, keeping its data and CCSID as stored. The data did not pass through this computer.",
        "None needed.",
    ),
    d(
        "QAD8006",
        30,
        "Deleting &1 stopped at &2: &3.",
        "The folder &1 is deleted from the inside out, one entry at a time. The system refused to delete &2, with the message shown (&3), so the delete stopped there. Entries deleted before it stay deleted; &2, the folders that hold it and anything not reached yet are still there.",
        "Correct what the system's message describes (for example, a file checked out to another user, or one you are not authorized to delete), then delete &1 again.",
    ),
    d(
        "QAD8010",
        10,
        "&1 already exists in &2.",
        "An entry named &1 is already in the folder &2, so the copy, move, upload or download would replace it.",
        "Choose Replace to replace it, Skip to leave it as it is, or Keep both (downloads only) to save the new file with a number added to its name.",
    ),
    d(
        "QAD8011",
        10,
        "&1 changed on the system since you opened it.",
        "The file &1 was changed on the system after this editor read it: its change count (on QAP version 6; before it, its last-changed time) is not the one it had when it was opened or last saved. Saving now would replace those changes.",
        "Choose Overwrite to save your text over the system's file, Reload to read the system's file again and lose your changes, or Cancel to go on editing.",
    ),
    d(
        "QAD8012",
        20,
        "&1 is too large to edit here; download it instead.",
        "The file &1 is larger than 64 MiB, the most the built-in editor opens.",
        "Download the file, then open it with a program on this computer that edits large files, and upload it again.",
    ),
    d(
        "QAD8013",
        10,
        "&1 is not a text file; opened read-only.",
        "The file &1 is binary (its CCSID is 65535, or its first 8 KiB hold NUL bytes), or it is larger than 8 MiB, so the editor shows it without letting it be changed.",
        "To change the file, download it, edit it with a program that suits it, and upload it again.",
    ),
    d(
        "QAD8020",
        0,
        "Transfer of &1 complete: &2 in &3.",
        "The file &1 was copied; &2 of data took &3.",
        "None needed.",
    ),
    d(
        "QAD8021",
        20,
        "Transfer of &1 failed: &2.",
        "The file &1 was not copied completely. The reason given was: &2. A download keeps its partial data in a temporary file beside the target; the target itself is unchanged.",
        "Correct the cause, then select Retry in the transfer list. A binary transfer, and a text download, resume from the last part the system confirmed; a text upload starts again from the beginning.",
    ),
    d(
        "QAD8022",
        10,
        "Transfer of &1 cancelled.",
        "The transfer of &1 was cancelled before it ended. A download keeps its partial data in a temporary file beside the target; the target itself is unchanged.",
        "Select Retry in the transfer list to go on from where it stopped, or Remove finished to drop it from the list.",
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
