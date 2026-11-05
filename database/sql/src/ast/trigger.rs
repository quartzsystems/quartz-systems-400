// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CREATE TRIGGER`.

use super::{Ident, ObjectName, PlExpr, PlStmt};

/// When a trigger fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerTime {
    /// `BEFORE` (`NO CASCADE BEFORE`).
    Before,
    /// `AFTER`.
    After,
    /// `INSTEAD OF`.
    InsteadOf,
}

/// What a trigger fires for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerEvent {
    /// `INSERT`.
    Insert,
    /// `DELETE`.
    Delete,
    /// `UPDATE [OF columns]`.
    Update(Vec<Ident>),
}

/// `CREATE [OR REPLACE] TRIGGER`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateTrigger {
    /// `OR REPLACE`.
    pub replace: bool,
    /// The trigger.
    pub name: ObjectName,
    /// When.
    pub time: TriggerTime,
    /// What.
    pub event: TriggerEvent,
    /// `ON`: the table or view.
    pub table: ObjectName,
    /// `REFERENCING OLD [ROW] AS`.
    pub old_row: Option<Ident>,
    /// `REFERENCING NEW [ROW] AS`.
    pub new_row: Option<Ident>,
    /// `REFERENCING OLD TABLE AS`.
    pub old_table: Option<Ident>,
    /// `REFERENCING NEW TABLE AS`.
    pub new_table: Option<Ident>,
    /// `FOR EACH STATEMENT` (else `ROW`).
    pub statement: bool,
    /// `MODE DB2SQL` (`Some(true)`) or `DB2ROW`; `None` when not given.
    pub db2sql: Option<bool>,
    /// `SET OPTION`.
    pub options: Vec<(String, String)>,
    /// `WHEN (condition)`.
    pub when: Option<PlExpr>,
    /// The triggered statement.
    pub body: Box<PlStmt>,
    /// Its text as written.
    pub body_text: String,
}
