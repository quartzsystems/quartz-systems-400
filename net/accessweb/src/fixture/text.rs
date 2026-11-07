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
