// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Transfers: `RETR`, `STOR`, `APPE`, `STOU`, the data connection's
//! stream and block modes, `ABOR` while data moves, `REST`, and the
//! files `STOR` makes when a member's file is not there.

use ifs::Errno;
use ifs::vfs::qsys::QsysNode;
use regfac::formats::tcp::op;

use super::Session;
use super::dirs::describe;
use crate::host::Host;
use crate::names;
use crate::net::{DataConn, Net};
use crate::objects::{self, Kind};
use crate::reply::Reply;
use crate::store::{self, ended};

/// Bytes moved between looks at the control connection.
const SLICE: usize = 64 * 1024;
/// Block mode: the end-of-file descriptor bit.
const EOF: u8 = 0x40;

/// What `STOR`, `APPE` and `STOU` do with what is there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Store {
    /// Replace it.
    Replace,
    /// Add to it.
    Append,
    /// Pick a name not in use.
    Unique,
}

/// How a transfer ended.
enum Moved {
    /// All of it.
    Done(Vec<u8>),
    /// The client sent `ABOR`.
    Aborted,
    /// The data connection failed.
    Failed(String),
}

/// Write `data` in stream or block mode, a slice at a time, looking for
/// `ABOR` between slices.
fn write_out(conn: &mut dyn DataConn, data: &[u8], block: bool, net: &mut dyn Net) -> Moved {
    for chunk in data.chunks(SLICE) {
        if net.aborted() {
            return Moved::Aborted;
        }
        let r = if block {
            let mut b = Vec::with_capacity(chunk.len() + 3);
            for piece in chunk.chunks(0xFFFF) {
                b.push(0);
                b.extend_from_slice(&u16::try_from(piece.len()).unwrap_or(0xFFFF).to_be_bytes());
                b.extend_from_slice(piece);
            }
            conn.write_all(&b)
        } else {
            conn.write_all(chunk)
        };
        if let Err(e) = r {
            return Moved::Failed(e.to_string());
        }
    }
    if block && let Err(e) = conn.write_all(&[EOF, 0, 0]) {
        return Moved::Failed(e.to_string());
    }
    Moved::Done(Vec::new())
}

/// Read to the end (stream) or to the block marked end of file.
fn read_in(conn: &mut dyn DataConn, block: bool, net: &mut dyn Net) -> Moved {
    let mut raw = Vec::new();
    let mut buf = vec![0u8; SLICE];
    loop {
        if net.aborted() {
            return Moved::Aborted;
        }
        match conn.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Moved::Failed(e.to_string()),
        }
        if block && blocks_done(&raw) {
            break;
        }
    }
    if !block {
        return Moved::Done(raw);
    }
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i + 3 <= raw.len() {
        let desc = raw[i];
        let len = usize::from(u16::from_be_bytes([raw[i + 1], raw[i + 2]]));
        let end = (i + 3 + len).min(raw.len());
        // Restart markers (descriptor 0x10) carry no data.
        if desc & 0x10 == 0 {
            out.extend_from_slice(&raw[i + 3..end]);
        }
        i = end;
        if desc & EOF != 0 {
            break;
        }
    }
    Moved::Done(out)
}

/// Whether block-mode data holds its last block.
fn blocks_done(raw: &[u8]) -> bool {
    let mut i = 0;
    while i + 3 <= raw.len() {
        let desc = raw[i];
        let len = usize::from(u16::from_be_bytes([raw[i + 1], raw[i + 2]]));
        if i + 3 + len > raw.len() {
            return false;
        }
        if desc & EOF != 0 {
            return true;
        }
        i += 3 + len;
    }
    false
}

impl<H: Host> Session<H> {
    /// Send `data` on the data connection: `prelim` first (a client may
    /// wait for it before it connects), `done` (returned) once all went.
    pub(super) fn send_data(&mut self, net: &mut dyn Net, prelim: &Reply, data: &[u8], done: Reply) -> Reply {
        self.say(net, prelim);
        let mut conn = match net.data() {
            Ok(c) => c,
            Err(e) => return Reply::new(425, format!("The data connection could not be opened: {e}.")),
        };
        let moved = write_out(conn.as_mut(), data, self.block, net);
        let fin = conn.finish();
        match moved {
            Moved::Aborted => {
                self.say(net, &Reply::new(426, "Transfer aborted; the data connection is closed."));
                Reply::new(226, "ABOR done.")
            }
            Moved::Failed(e) => Reply::new(426, format!("The data connection failed: {e}.")),
            Moved::Done(_) => match fin {
                Ok(()) => done,
                Err(e) => Reply::new(426, format!("The data connection failed: {e}.")),
            },
        }
    }

    /// Take the client's data: `prelim` first, then the connection.
    fn receive(&mut self, net: &mut dyn Net, prelim: &Reply) -> Result<Vec<u8>, Reply> {
        self.say(net, prelim);
        let mut conn = match net.data() {
            Ok(c) => c,
            Err(e) => return Err(Reply::new(425, format!("The data connection could not be opened: {e}."))),
        };
        let moved = read_in(conn.as_mut(), self.block, net);
        let _ = conn.finish();
        match moved {
            Moved::Done(d) => Ok(d),
            Moved::Aborted => {
                self.say(net, &Reply::new(426, "Transfer aborted; nothing was stored."));
                Err(Reply::new(226, "ABOR done."))
            }
            Moved::Failed(e) => Err(Reply::new(426, format!("The data connection failed: {e}."))),
        }
    }

    /// Why a missing member or file cannot be found.
    pub(super) fn missing(&mut self, path: &str, e: Errno) -> Reply {
        store::missing(&mut self.host, path, e)
    }

    /// The form data takes in this session.
    pub(super) fn form(&self) -> store::Form {
        store::Form {
            rep: self.rep,
            codes: self.codes(),
            trim: self.trim != 0,
            crtccsid: self.crtccsid,
            user_ccsid: self.user.as_ref().map_or(0, |p| p.ccsid),
        }
    }

    /// Block mode and record structure take members only.
    fn mode_refused(&self, kind: &Kind) -> Option<Reply> {
        let stream = matches!(kind, Kind::Stream);
        if stream && self.block {
            return Some(Reply::new(504, "Block mode is for database members; use MODE S for stream files."));
        }
        if stream && self.record {
            return Some(Reply::new(504, "Record structure is for database members; use STRU F for stream files."));
        }
        None
    }

    /// An object's bytes as `RETR` sends them in the current type.
    pub(super) fn fetch(&mut self, path: &str) -> Result<Vec<u8>, Reply> {
        if !names::in_qsys(path)
            && let Some(r) = self.mode_refused(&Kind::Stream)
        {
            return Err(r);
        }
        if let Ok((k, _)) = objects::look(self.host.ifs(), path)
            && let Some(r) = self.mode_refused(&k)
        {
            return Err(r);
        }
        let form = self.form();
        store::fetch(&mut self.host, &form, path)
    }

    pub(super) fn retr(&mut self, arg: &str, rest: u64, net: &mut dyn Net) -> Reply {
        let path = match self.object_path(arg) {
            Ok(p) => p,
            Err(r) => return r,
        };
        if !self.allowed(net, op::SEND_FILE, &path) {
            return Reply::new(550, format!("An exit program refused to send {}.", describe(&path)));
        }
        let data = match self.fetch(&path) {
            Ok(d) => d,
            Err(r) => return r,
        };
        let from = usize::try_from(rest).unwrap_or(usize::MAX).min(data.len());
        let what = describe(&path);
        self.send_data(
            net,
            &Reply::new(150, format!("Sending {what}.")),
            &data[from..],
            Reply::new(226, "Transfer complete."),
        )
    }

    /// A name `STOU` may use: the name itself, else with 1, 2, ... after.
    fn unique(&mut self, path: &str) -> String {
        if !objects::exists(self.host.ifs(), path) {
            return path.to_string();
        }
        if let Some(QsysNode::Mbr(l, f, m)) = names::qsys_node(path) {
            for n in 1..1000 {
                let base: String = m.chars().take(10 - n.to_string().len()).collect();
                let p = objects::member_path(&l, &f, &format!("{base}{n}"));
                if !objects::exists(self.host.ifs(), &p) {
                    return p;
                }
            }
        }
        for n in 1.. {
            let p = format!("{path}{n}");
            if !objects::exists(self.host.ifs(), &p) {
                return p;
            }
        }
        path.to_string()
    }

    /// Check that `STOR` can write `path` before data moves.
    fn can_store(&mut self, path: &str) -> Result<(), Reply> {
        match objects::look(self.host.ifs(), path) {
            Ok((k, _)) if k.is_dir() && !matches!(k, Kind::File(..)) => {
                Err(Reply::new(550, format!("{} is a directory or library.", describe(path))))
            }
            Ok((k, _)) => self.mode_refused(&k).map_or(Ok(()), Err),
            Err(_) => match names::qsys_node(path) {
                Some(QsysNode::Mbr(l, ..) | QsysNode::Obj(l, ..)) => {
                    if objects::exists(self.host.ifs(), &names::library_dir(&l)) {
                        Ok(())
                    } else {
                        Err(Reply::new(550, format!("Library {l} not found.")))
                    }
                }
                Some(_) => Err(Reply::new(550, format!("{} cannot be stored into.", describe(path)))),
                None => {
                    if let Some(r) = self.mode_refused(&Kind::Stream) {
                        return Err(r);
                    }
                    let parent = names::normalize(&format!("{path}/.."));
                    match objects::look(self.host.ifs(), &parent) {
                        Ok((Kind::Dir, _)) => Ok(()),
                        _ => Err(ended(426, format!("{parent} cannot hold the file to be received."))),
                    }
                }
            },
        }
    }

    pub(super) fn stor(&mut self, arg: &str, rest: u64, how: Store, net: &mut dyn Net) -> Reply {
        let arg = if how == Store::Unique && arg.trim().is_empty() { "QSFTPFILE" } else { arg };
        let mut path = match self.object_path(arg) {
            Ok(p) => p,
            Err(r) => return r,
        };
        if let Some(QsysNode::Obj(l, f, t)) = names::qsys_node(&path)
            && t == "*FILE"
        {
            path = objects::member_path(&l, &f, &f);
        }
        if how == Store::Unique {
            path = self.unique(&path);
        }
        if !self.allowed(net, op::RECEIVE_FILE, &path) {
            return Reply::new(550, format!("An exit program refused to store {}.", describe(&path)));
        }
        if let Err(r) = self.can_store(&path) {
            return r;
        }
        let what = describe(&path);
        let data = match self.receive(net, &Reply::new(150, format!("Receiving {what}."))) {
            Ok(d) => d,
            Err(r) => return r,
        };
        match self.put(&path, &data, how == Store::Append, rest) {
            Ok(false) => Reply::new(226, "Transfer complete."),
            Ok(true) => ended(426, format!("Lines longer than the records of {what} were cut to fit.")),
            Err(e) => ended(426, format!("{what} not stored: {e}.")),
        }
    }

    /// Write received data to `path`; whether lines were cut to fit
    /// a member's records.
    fn put(&mut self, path: &str, data: &[u8], append: bool, rest: u64) -> Result<bool, String> {
        let form = self.form();
        store::put(&mut self.host, &form, path, data, append, rest)
    }

    /// Move a member to another file of the library (`RNTO`).
    pub(super) fn move_member(&mut self, l: &str, f: &str, m: &str, f2: &str, m2: &str) -> Result<(), String> {
        store::move_member(&mut self.host, l, f, m, f2, m2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::mem::MemNet;

    #[test]
    fn block_mode_round_trip() {
        let mut net = MemNet::default();
        net.passive(false).unwrap();
        let mut c = net.data().unwrap();
        let data: Vec<u8> = (0..70_000u32).map(|i| u8::try_from(i % 251).unwrap()).collect();
        assert!(matches!(write_out(c.as_mut(), &data, true, &mut net), Moved::Done(_)));
        c.finish().unwrap();
        drop(c);
        let framed = net.take_sent().remove(0);
        assert!(blocks_done(&framed));
        net.uploads.push_back(framed);
        let mut c = net.data().unwrap();
        match read_in(c.as_mut(), true, &mut net) {
            Moved::Done(d) => assert_eq!(d, data),
            _ => panic!("not read"),
        }
    }
}
