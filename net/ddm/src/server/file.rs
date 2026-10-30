// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The System/38 file commands over the `FileBackend`. An open file is
//! known by the requester's `DCLNAM`. Reads answer one object DSS with
//! `S38BUF` and `S38IOFB`; the end of data or a missing record answers
//! `S38IOFB` and an `S38MSGRM` (`CPF5001`, `CPF5006`); writes, updates
//! and deletes answer `S38IOFB`; close and commands answer `S38MSGRM`.

use super::backend::{FileError, FileInfo, FileOpen, ReadAt};
use super::Session;
use crate::cp::{param, s38, svrcod};
use crate::file::{self, Ctl, Iofb, OpenFeedback, Optl, Rec, Ufcb, ufcb};
use crate::wire::{DssType, Object, text};

/// One open file.
#[derive(Debug)]
pub struct Open {
    handle: u32,
    info: FileInfo,
    blocking: u16,
}

type Out = Vec<(DssType, Object)>;

fn error(e: &FileError) -> (DssType, Object) {
    (
        DssType::Reply,
        file::msgrm(&file::Message {
            severity: svrcod::ERROR,
            id: e.id.clone(),
            text: e.text.clone(),
        }),
    )
}

fn done() -> (DssType, Object) {
    (
        DssType::Reply,
        file::msgrm(&file::Message {
            severity: svrcod::INFO,
            id: String::new(),
            text: String::new(),
        }),
    )
}

impl Session {
    fn iofb(o: &Open, count: u16, rrn: u32) -> Object {
        Iofb {
            format: o.info.format.clone(),
            reclen: u32::from(o.info.reclen),
            count,
            rrn,
        }
        .to_object()
    }

    pub(super) fn file_command(&mut self, c: &Object, data: &[Object]) -> Out {
        let name = c.get(param::DCLNAM).map(|d| text(d.bytes())).unwrap_or_default();
        match c.cp {
            s38::S38CMD => {
                let cmd = c.get(param::S38CMDST).map(|d| text(d.bytes())).unwrap_or_default();
                match self.files.command(&cmd) {
                    Ok(msgs) if msgs.is_empty() => vec![done()],
                    Ok(msgs) => msgs
                        .iter()
                        .map(|m| {
                            (
                                DssType::Reply,
                                file::msgrm(&file::Message {
                                    severity: svrcod::INFO,
                                    id: m.id.clone(),
                                    text: m.text.clone(),
                                }),
                            )
                        })
                        .collect(),
                    Err(e) => vec![error(&e)],
                }
            }
            s38::S38OPEN => {
                let Some(u) = c.get(param::S38UFCB).and_then(|u| Ufcb::decode(u.bytes()).ok()) else {
                    return vec![(DssType::Reply, crate::msg::reply::syntaxrm(0x0E, param::S38UFCB))];
                };
                let req = FileOpen {
                    file: u.file.clone(),
                    library: u.library.clone(),
                    member: u.member.clone(),
                    input: u.flags & ufcb::INPUT != 0,
                    output: u.flags & ufcb::OUTPUT != 0,
                    update: u.flags & ufcb::UPDATE != 0,
                    delete: u.flags & ufcb::DELETE != 0,
                    keyed: !u.arrival,
                };
                match self.files.open(&req) {
                    Ok((handle, info)) => {
                        let fb = OpenFeedback {
                            input: req.input,
                            output: req.output,
                            file: info.file.clone(),
                            library: info.library.clone(),
                            member: info.member.clone(),
                            reclen: info.reclen,
                            records: info.records,
                            keyed: info.keyed && req.keyed,
                            blocking: u.blocking.max(1),
                            ccsid: info.ccsid,
                        };
                        let o = Open {
                            handle,
                            info,
                            blocking: u.blocking.max(1),
                        };
                        let iofb = Self::iofb(&o, 0, 0);
                        self.opened.insert(name, o);
                        vec![(DssType::Object, fb.to_object()), (DssType::Object, iofb)]
                    }
                    Err(e) => vec![error(&e)],
                }
            }
            s38::S38CLOSE => match self.opened.remove(&name) {
                Some(o) => match self.files.close(o.handle) {
                    Ok(()) => vec![done()],
                    Err(e) => vec![error(&e)],
                },
                None => vec![error(&not_open())],
            },
            _ => {
                let Some(o) = self.opened.get(&name) else {
                    return vec![error(&not_open())];
                };
                let (h, blocking, reclen, fields) = (
                    o.handle,
                    o.blocking,
                    usize::from(o.info.reclen),
                    usize::from(o.info.fields),
                );
                let optl = Optl::of(c).unwrap_or(Optl {
                    op: file::op::NEXT,
                    lock: 0,
                    opts: 0,
                });
                let ctll = c
                    .get(param::S38CTLL)
                    .and_then(|x| file::decode_ctll(x.bytes()).ok())
                    .unwrap_or_default();
                let recs = data
                    .iter()
                    .find(|d| d.cp == param::S38BUF)
                    .and_then(|b| file::decode_buf(b.bytes(), reclen, fields).ok())
                    .unwrap_or_default();
                self.file_op(c.cp, &name, h, blocking, optl, &ctll, &recs)
            }
        }
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn file_op(
        &mut self,
        cp: u16,
        name: &str,
        h: u32,
        blocking: u16,
        optl: Optl,
        ctll: &[Ctl],
        recs: &[Rec],
    ) -> Out {
        let obj = |o: Object| (DssType::Object, o);
        let o = &self.opened[name];
        let reply_iofb = |count: u16, rrn: u32| obj(Self::iofb(o, count, rrn));
        match cp {
            s38::S38GET | s38::S38GETK | s38::S38GETM => {
                let lock = optl.lock == file::lock::UPDATE;
                let want = match cp {
                    s38::S38GETM => ctll
                        .iter()
                        .find_map(|c| match c {
                            Ctl::Count(n) => Some(*n),
                            _ => None,
                        })
                        .unwrap_or(blocking),
                    s38::S38GETK => 1,
                    _ if lock => 1,
                    _ => blocking,
                };
                let mut first = match (cp, optl.op) {
                    (s38::S38GETK, _) => {
                        let key = ctll.iter().find_map(|c| match c {
                            Ctl::Key(k) => Some(k.clone()),
                            _ => None,
                        });
                        let fields = ctll
                            .iter()
                            .find_map(|c| match c {
                                Ctl::KeyFields(n) => Some(*n),
                                _ => None,
                            })
                            .unwrap_or(1);
                        ReadAt::Key {
                            key: key.unwrap_or_default(),
                            fields,
                        }
                    }
                    (_, file::op::FIRST) => ReadAt::First,
                    _ => ReadAt::Next,
                };
                let format = o.info.format.clone();
                let iofb_of = |count: u16, rrn: u32, reclen: u16| {
                    Iofb {
                        format: format.clone(),
                        reclen: u32::from(reclen),
                        count,
                        rrn,
                    }
                    .to_object()
                };
                let reclen = o.info.reclen;
                let mut got = Vec::new();
                for _ in 0..want.max(1) {
                    match self.files.read(h, &first, lock) {
                        Ok(Some(r)) => got.push(Rec {
                            rrn: r.rrn,
                            flags: 1,
                            locked: lock,
                            nulls: r.nulls,
                            data: r.data,
                        }),
                        Ok(None) => break,
                        Err(e) => return vec![error(&e)],
                    }
                    if cp == s38::S38GETK {
                        break;
                    }
                    first = ReadAt::Next;
                }
                if got.is_empty() {
                    let msg = if cp == s38::S38GETK {
                        file::msgrm(&file::Message {
                            severity: svrcod::ERROR,
                            id: "CPF5006".into(),
                            text: "No record has that key.".into(),
                        })
                    } else {
                        file::eof_msgrm()
                    };
                    return vec![obj(iofb_of(0, 0, reclen)), (DssType::Reply, msg)];
                }
                let last = got.last().map_or(0, |r| r.rrn);
                let n = u16::try_from(got.len()).unwrap_or(u16::MAX);
                vec![obj(file::buf(&got)), obj(iofb_of(n, last, reclen))]
            }
            s38::S38PUTM => {
                let mut last = 0;
                for r in recs {
                    match self.files.write(h, &r.data, &r.nulls) {
                        Ok(rrn) => last = rrn,
                        Err(e) => return vec![error(&e)],
                    }
                }
                vec![reply_iofb(u16::try_from(recs.len()).unwrap_or(0), last)]
            }
            s38::S38UPDAT => {
                let Some(r) = recs.first() else {
                    return vec![error(&not_open())];
                };
                match self.files.update(h, &r.data, &r.nulls) {
                    Ok(()) => vec![reply_iofb(1, 0)],
                    Err(e) => vec![error(&e)],
                }
            }
            s38::S38DEL => match self.files.delete(h) {
                Ok(()) => vec![reply_iofb(1, 0)],
                Err(e) => vec![error(&e)],
            },
            s38::S38FEOD => match self.files.feod(h) {
                Ok(()) => vec![reply_iofb(0, 0), (DssType::Reply, file::eof_msgrm())],
                Err(e) => vec![error(&e)],
            },
            other => vec![(DssType::Reply, crate::msg::reply::cmdnsprm(other))],
        }
    }
}

fn not_open() -> FileError {
    FileError {
        id: "CPF4132".into(),
        text: "No file is open under that name.".into(),
    }
}
