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
    // The database's (QSQLMSG), as the system words them.
    ("SQL0100", 0, "No row was found for &1.", "The statement found no row to work on; nothing was changed.", "None."),
    ("SQL0104", 30, "Syntax error at line &1 column &2: &3.", "The statement could not be parsed at the position shown.", "Correct the statement and run it again."),
    ("SQL0181", 30, "A date, time or timestamp string has a part out of range.", "The string has the right form but a month, day, hour, minute or second that does not exist.", "Correct the value."),
    ("SQL0206", 30, "Column &1 not found.", "No table in the statement has a column of that name.", "Correct the name and run the statement again."),
    ("SQL0302", 30, "A value is not valid for its target: &1.", "A literal or host value could not be converted to the column's type.", "Correct the value."),
    ("SQL0404", 30, "A value for &1 is longer than the column allows.", "The string is longer than the column's length.", "Shorten the value or widen the column."),
    ("SQL0407", 30, "Column &1 does not allow nulls.", "A null value was assigned to a column defined without ALWNULL.", "Give the column a value."),
    ("SQL0501", 30, "Cursor &1 is not open.", "FETCH, UPDATE or DELETE WHERE CURRENT OF needs an open cursor.", "Run the query again."),
    ("SQL0803", 30, "Duplicate key value in &1.", "The row would repeat a key of a unique index or the primary key.", "Change the key value."),
    ("SQL0901", 50, "SQL system error: &1.", "The database layer reported an error the SQL executor did not expect.", "Report the problem."),
    ("SQL0204", 30, "&1 in &2 not found.", "No table, view or index of that name exists in the schema named or the current schema.", "Correct the name and run the statement again."),
    ("SQL0752", 30, "The connection's SQL options cannot change while a transaction is open.", "Changes made under commitment control have not been committed or rolled back.", "Commit or roll back, then set the options again."),
    ("SQL7955", 0, "&3 rows were deleted from &1 in &2.", "The DELETE statement ran.", "None."),
    ("SQL7956", 0, "&3 rows were inserted into &1 in &2.", "The INSERT statement ran.", "None."),
    ("SQL7957", 0, "&3 rows were updated in &1 in &2.", "The UPDATE statement ran.", "None."),
    ("SQL7958", 0, "&1 rows fetched in &2 seconds.", "The query ran and its rows were read.", "None."),
    ("SQL7977", 0, "The &1 statement ran.", "The statement ran without an error.", "None."),
    ("SQL7960", 0, "The commit is done.", "The changes of the unit of work are kept.", "None."),
    ("SQL7961", 0, "The rollback is done.", "The changes of the unit of work are undone.", "None."),
    // Files and Print (steps 13-15).
    ("CPFA09C", 40, "Not enough authority for &1.", "You do not have the authority the operation needs to the object, or to a directory in its path.", "Ask its owner or your security officer for authority."),
    ("CPFA0A0", 40, "&1 exists already.", "An object of that name exists in the directory.", "Choose another name, or remove the existing one first."),
    ("CPFA0A9", 40, "&1 was not found.", "No object has the path name given.", "Check the name with WRKLNK."),
    ("CPFA0AC", 40, "Directory &1 is not empty.", "A directory must be empty before it is removed, unless the command removes its contents too.", "Remove what it holds first."),
    ("CPFA0DA", 40, "&1 is a directory.", "The operation needs an object that is not a directory.", "Name a file."),
    ("CPF3303", 40, "&1 is not authorized to output queue &2.", "Working with another user's spooled files needs special authority *SPLCTL or *JOBCTL, or authority to the output queue.", "Ask your security officer for the authority."),
    ("CPF3309", 40, "No spooled file &1.", "No spooled file has the name, job and number given.", "Refresh the list and try again."),
    ("CPF3330", 40, "Spooled file &1 is being printed.", "A file being printed cannot be moved.", "Hold it first, or wait until it has printed."),
    ("CPI9801", 0, "Search string &1 not found.", "No page of the spooled file holds the text.", "Search for other text."),
    ("CPF3337", 40, "Spooled file not released: it is neither held nor saved.", "Only a held or saved spooled file can be released.", "None."),
    // Messages, jobs and commands.
    (
        "CPA0702",
        99,
        "&1 received by procedure &2. (C D I R)",
        "The procedure sent an escape message it did not handle, and the program asks the operator what to do.",
        "Reply C to cancel, D to cancel with a dump, I to ignore the message or R to retry.",
    ),
    (
        "CPF1241",
        0,
        "Job &3/&2/&1 completed normally.",
        "The job ran to its end without an error it left unhandled.",
        "None.",
    ),
    (
        "CPC1221",
        0,
        "Job &3/&2/&1 submitted to job queue &4 in &5.",
        "The job waits on the job queue until a subsystem that holds the queue starts it.",
        "None.",
    ),
    (
        "CPC2191",
        0,
        "Object &1 in &2 of type &3 changed.",
        "The object's description was changed as the command asked.",
        "None.",
    ),
    (
        "CPI2101",
        0,
        "Job &1 submitted to job queue &2.",
        "The job waits on the job queue until a subsystem that holds the queue starts it.",
        "None.",
    ),
    (
        "CPF9801",
        40,
        "Object &1 in library &2 was not found.",
        "No object of that name and type is in the library named.",
        "Correct the object or library name and try the request again.",
    ),
    (
        "CPF0001",
        30,
        "Errors were found on command &1.",
        "The diagnostic messages before this one say what was wrong. The command did not run.",
        "Correct the errors the messages name and run the command again.",
    ),
    (
        "CPD0030",
        30,
        "Command &1 in library &2 was not found.",
        "No *CMD object of that name is in the library named, or in the library list when none was named.",
        "Correct the command name or library and try again.",
    ),
    (
        "CPF2410",
        40,
        "Message key &1 not found.",
        "No message with that key is on the message queue: it may have been removed.",
        "Display the queue again and choose a message that is on it.",
    ),
    (
        "CPF2403",
        40,
        "No message queue &1 is in library &2.",
        "The message queue named does not exist.",
        "Check the queue's name and library, or create the queue with CRTMSGQ.",
    ),
    (
        "CPF1070",
        40,
        "Job &1 not found.",
        "No job of that qualified name is in the system.",
        "Correct the job name and try the request again.",
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
