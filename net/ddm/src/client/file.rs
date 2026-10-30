// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Record-level access on a connection, as a DDM file (`CRTDDMF`) uses
//! it: open a remote member, read first, next, by key or several, add,
//! update and delete records, force the end of data, close, and run
//! commands on the remote system.

use std::io::{Read, Write};

use super::{Connection, reply_error};
use crate::cp::{param, rm, s38};
use crate::file::{self, Ctl, Iofb, OpenFeedback, Optl, Rec, Ufcb};
use crate::wire::{Dss, Object};
use crate::Error;

/// A remote file opened on a connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteFile {
    /// The declared name.
    pub name: String,
    /// The open feedback.
    pub feedback: OpenFeedback,
    /// The record format.
    pub format: String,
    /// Fields in the format (the null map's length).
    pub fields: usize,
}

/// What a file operation answered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileReply {
    /// Records read.
    pub records: Vec<Rec>,
    /// The I/O feedback.
    pub iofb: Option<Iofb>,
    /// IBM i messages.
    pub messages: Vec<file::Message>,
}

impl FileReply {
    /// Whether a message reports the end of data or a missing record.
    pub fn at_end(&self) -> bool {
        self.messages.iter().any(|m| m.id == "CPF5001" || m.id == "CPF5006")
    }

    /// The first error message (severity 8 or more).
    pub fn error(&self) -> Option<&file::Message> {
        self.messages.iter().find(|m| m.severity >= 8)
    }
}

impl<S: Read + Write> Connection<S> {
    fn file_exchange(&mut self, list: Vec<Dss>, reclen: usize, fields: usize) -> Result<FileReply, Error> {
        let r = self.exchange(list)?;
        let mut out = FileReply::default();
        for d in &r {
            for o in d.objects() {
                match o.cp {
                    param::S38BUF => out.records = file::decode_buf(o.bytes(), reclen, fields)?,
                    param::S38IOFB => out.iofb = Some(Iofb::decode(o.bytes())?),
                    param::S38OPNFB => {}
                    rm::S38MSGRM => out.messages.push(file::message_of(o)),
                    _ => return Err(reply_error(o)),
                }
            }
        }
        Ok(out)
    }

    /// Open a member under declared name `name`; `fields` is the number
    /// of fields of the record format.
    pub fn open_file(&mut self, name: &str, u: &Ufcb, fields: usize) -> Result<RemoteFile, Error> {
        let corr = self.next_corr();
        let c = Object::coll(
            s38::S38OPEN,
            vec![file::dclnam(name), Object::data(param::S38UFCB, u.encode())],
        );
        let r = self.exchange(vec![Dss::request(corr, c)])?;
        let mut fb = None;
        for d in &r {
            for o in d.objects() {
                match o.cp {
                    param::S38OPNFB => fb = Some(OpenFeedback::decode(o.bytes())?),
                    param::S38IOFB => {}
                    rm::S38MSGRM => {
                        let m = file::message_of(o);
                        return Err(Error::Reply {
                            name: format!("{} {}", m.id, m.text),
                            svrcod: m.severity,
                        });
                    }
                    _ => return Err(reply_error(o)),
                }
            }
        }
        Ok(RemoteFile {
            name: name.into(),
            feedback: fb.ok_or(Error::Missing("S38OPNFB"))?,
            format: u.format.clone(),
            fields,
        })
    }

    /// Read: `op` from `file::op`, `lock` from `file::lock`, the control
    /// list for a key or a count.
    pub fn get(&mut self, f: &RemoteFile, cp: u16, op: u8, lock: u8, ctll: &[Ctl]) -> Result<FileReply, Error> {
        let corr = self.next_corr();
        let optl = Optl { op, lock, opts: if lock == 0 { 1 } else { 3 } };
        let c = file::op_cmd(cp, &f.name, optl, ctll);
        self.file_exchange(vec![Dss::request(corr, c)], usize::from(f.feedback.reclen), f.fields)
    }

    /// Read by key.
    pub fn get_key(&mut self, f: &RemoteFile, key: &[u8], key_fields: u32, lock: bool) -> Result<FileReply, Error> {
        let ctll = [
            Ctl::Format(f.format.clone()),
            Ctl::KeyFields(key_fields),
            Ctl::Key(key.to_vec()),
        ];
        self.get(f, s38::S38GETK, file::op::KEY_EQUAL, if lock { file::lock::UPDATE } else { 0 }, &ctll)
    }

    /// Add records.
    pub fn put(&mut self, f: &RemoteFile, recs: &[Rec]) -> Result<FileReply, Error> {
        let corr = self.next_corr();
        self.file_exchange(
            vec![
                Dss::request(corr, file::name_cmd(s38::S38PUTM, &f.name)),
                Dss::object(corr, file::buf(recs)),
            ],
            usize::from(f.feedback.reclen),
            f.fields,
        )
    }

    /// Update the record last read for update.
    pub fn update_rec(&mut self, f: &RemoteFile, rec: &Rec) -> Result<FileReply, Error> {
        let corr = self.next_corr();
        let optl = Optl { op: file::op::UPDATE, lock: file::lock::UPDATE, opts: 7 };
        self.file_exchange(
            vec![
                Dss::request(corr, file::op_cmd(s38::S38UPDAT, &f.name, optl, &[])),
                Dss::object(corr, file::buf(std::slice::from_ref(rec))),
            ],
            usize::from(f.feedback.reclen),
            f.fields,
        )
    }

    /// Delete the record last read for update.
    pub fn delete_rec(&mut self, f: &RemoteFile) -> Result<FileReply, Error> {
        let corr = self.next_corr();
        self.file_exchange(
            vec![Dss::request(corr, file::name_cmd(s38::S38DEL, &f.name))],
            usize::from(f.feedback.reclen),
            f.fields,
        )
    }

    /// Close.
    pub fn close_file(&mut self, f: &RemoteFile) -> Result<FileReply, Error> {
        let corr = self.next_corr();
        self.file_exchange(vec![Dss::request(corr, file::close(&f.name))], 0, 0)
    }

    /// Run a CL command on the remote system.
    pub fn remote_command(&mut self, command: &str) -> Result<FileReply, Error> {
        let corr = self.next_corr();
        self.file_exchange(vec![Dss::request(corr, file::cmd(command))], 0, 0)
    }
}
