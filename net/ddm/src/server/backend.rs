// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the application server drives: the SQL engine, the files
//! behind record-level access, and the password check. Each statement
//! is named by its package section; the engine keeps the prepared
//! statements and open cursors.

// The SQLCA is the error DRDA returns; it is not boxed.
#![allow(clippy::result_large_err)]

use crate::sec::Proof;
use crate::sql::{Column, Sqlca, Value};

/// An outcome or the SQLCA of the failure.
pub type SqlResult<T> = Result<T, Sqlca>;

/// The statement a section names: the package's collection and ID and
/// the section number.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Section {
    /// The collection.
    pub collection: String,
    /// The package.
    pub package: String,
    /// The section.
    pub number: u16,
}

/// The SQL engine.
pub trait SqlBackend {
    /// Prepare `sql` at `section`, giving its result columns (none for a
    /// statement that is not a query).
    fn prepare(&mut self, section: &Section, sql: &str) -> SqlResult<Vec<Column>>;
    /// The result columns of a prepared statement.
    fn describe(&mut self, section: &Section) -> SqlResult<Vec<Column>>;
    /// Run a prepared statement that is not a query; the SQLCA carries
    /// the row count.
    fn execute(&mut self, section: &Section, params: &[Value]) -> SqlResult<Sqlca>;
    /// Run a statement once.
    fn execute_immediate(&mut self, sql: &str) -> SqlResult<Sqlca>;
    /// Open the cursor of a prepared query.
    fn open(&mut self, section: &Section, params: &[Value]) -> SqlResult<Vec<Column>>;
    /// Up to `max` more rows; fewer (or none) at the end.
    fn fetch(&mut self, section: &Section, max: usize) -> SqlResult<Vec<Vec<Value>>>;
    /// Close the cursor.
    fn close(&mut self, section: &Section) -> SqlResult<()>;
    /// Commit.
    fn commit(&mut self) -> SqlResult<()>;
    /// Roll back.
    fn rollback(&mut self) -> SqlResult<()>;
}

/// How a file is opened.
#[allow(clippy::struct_excessive_bools)] // the four uses an open asks for
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileOpen {
    /// The file.
    pub file: String,
    /// The library (`*LIBL`).
    pub library: String,
    /// The member (`*FIRST`).
    pub member: String,
    /// Read.
    pub input: bool,
    /// Write.
    pub output: bool,
    /// Update.
    pub update: bool,
    /// Delete.
    pub delete: bool,
    /// Keyed access (else arrival sequence).
    pub keyed: bool,
}

/// What an open file is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileInfo {
    /// The file, library and member opened.
    pub file: String,
    /// The library.
    pub library: String,
    /// The member.
    pub member: String,
    /// The record format.
    pub format: String,
    /// The record length.
    pub reclen: u16,
    /// The number of fields.
    pub fields: u16,
    /// The records in the member.
    pub records: u32,
    /// Whether the access path is keyed.
    pub keyed: bool,
    /// The CCSID of the character fields.
    pub ccsid: u16,
}

/// Which record to read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadAt {
    /// The first.
    First,
    /// The next.
    Next,
    /// The record whose key (the first `fields` key fields, in the
    /// record's encoding) equals `key`.
    Key {
        /// The key bytes.
        key: Vec<u8>,
        /// How many key fields they cover.
        fields: u32,
    },
    /// By relative record number.
    Rrn(u32),
}

/// A record read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRecord {
    /// Its relative record number.
    pub rrn: u32,
    /// Its bytes.
    pub data: Vec<u8>,
    /// Null fields.
    pub nulls: Vec<bool>,
}

/// An IBM i message for a failed operation (`CPF5006`: not found).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileError {
    /// The message ID.
    pub id: String,
    /// Its text.
    pub text: String,
}

/// The files behind record-level access, one handle per open.
pub trait FileBackend {
    /// Open a file member.
    fn open(&mut self, req: &FileOpen) -> Result<(u32, FileInfo), FileError>;
    /// Read a record (`None` at the end or when no record qualifies),
    /// locking it for update when `lock`.
    fn read(&mut self, h: u32, at: &ReadAt, lock: bool) -> Result<Option<FileRecord>, FileError>;
    /// Add a record; its relative record number.
    fn write(&mut self, h: u32, data: &[u8], nulls: &[bool]) -> Result<u32, FileError>;
    /// Replace the record last read for update.
    fn update(&mut self, h: u32, data: &[u8], nulls: &[bool]) -> Result<(), FileError>;
    /// Delete the record last read for update.
    fn delete(&mut self, h: u32) -> Result<(), FileError>;
    /// Force the end of data (position at the end, write the buffers).
    fn feod(&mut self, h: u32) -> Result<(), FileError>;
    /// Close.
    fn close(&mut self, h: u32) -> Result<(), FileError>;
    /// Run a CL command (`S38CMD`); the messages it sent, the last one
    /// the escape when it failed.
    fn command(&mut self, cmd: &str) -> Result<Vec<FileError>, FileError>;
}

/// The password check.
pub trait Authenticator {
    /// Whether `proof` shows `user`'s password; a `SECCHKCD` when not.
    fn check(&mut self, user: &str, proof: &Proof) -> Result<(), u8>;
}

/// A backend that has nothing: every SQL statement and file fails.
#[derive(Debug, Default)]
pub struct Nothing;

fn unsupported() -> Sqlca {
    Sqlca::error(-30_090, "25000", "")
}

impl SqlBackend for Nothing {
    fn prepare(&mut self, _: &Section, _: &str) -> SqlResult<Vec<Column>> {
        Err(unsupported())
    }
    fn describe(&mut self, _: &Section) -> SqlResult<Vec<Column>> {
        Err(unsupported())
    }
    fn execute(&mut self, _: &Section, _: &[Value]) -> SqlResult<Sqlca> {
        Err(unsupported())
    }
    fn execute_immediate(&mut self, _: &str) -> SqlResult<Sqlca> {
        Err(unsupported())
    }
    fn open(&mut self, _: &Section, _: &[Value]) -> SqlResult<Vec<Column>> {
        Err(unsupported())
    }
    fn fetch(&mut self, _: &Section, _: usize) -> SqlResult<Vec<Vec<Value>>> {
        Err(unsupported())
    }
    fn close(&mut self, _: &Section) -> SqlResult<()> {
        Ok(())
    }
    fn commit(&mut self) -> SqlResult<()> {
        Ok(())
    }
    fn rollback(&mut self) -> SqlResult<()> {
        Ok(())
    }
}

impl FileBackend for Nothing {
    fn open(&mut self, req: &FileOpen) -> Result<(u32, FileInfo), FileError> {
        Err(FileError {
            id: "CPF4101".into(),
            text: format!("{}/{} does not exist here.", req.library, req.file),
        })
    }
    fn read(&mut self, _: u32, _: &ReadAt, _: bool) -> Result<Option<FileRecord>, FileError> {
        Ok(None)
    }
    fn write(&mut self, _: u32, _: &[u8], _: &[bool]) -> Result<u32, FileError> {
        Err(not_open())
    }
    fn update(&mut self, _: u32, _: &[u8], _: &[bool]) -> Result<(), FileError> {
        Err(not_open())
    }
    fn delete(&mut self, _: u32) -> Result<(), FileError> {
        Err(not_open())
    }
    fn feod(&mut self, _: u32) -> Result<(), FileError> {
        Ok(())
    }
    fn close(&mut self, _: u32) -> Result<(), FileError> {
        Ok(())
    }
    fn command(&mut self, _: &str) -> Result<Vec<FileError>, FileError> {
        Err(FileError {
            id: "CPF0006".into(),
            text: "This server runs no commands.".into(),
        })
    }
}

fn not_open() -> FileError {
    FileError {
        id: "CPF4132".into(),
        text: "No file is open under that name.".into(),
    }
}
