// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Listings: `LIST` and `NLST` in both naming and listing formats, as
//! the oracle shows them, and RFC 3659's `MLSD`, `MLST`, `SIZE` and
//! `MDTM`.
//!
//! In `NAMEFMT 1` a name in `LISTFMT 0` and in `NLST` is the path
//! relative to the current directory when it is under it, else the
//! whole path, a directory's with a slash after; `LISTFMT 1` shows the
//! last component only. In `NAMEFMT 0` a bare name is a library, `F.M`
//! and `F.*` list members, a generic name picks files of the current
//! library, and names in another library carry `LIB/`.

use ifs::ops::Stat;
use ifs::ops::flags::{S_IFLNK, S_IFMT};
use ifs::vfs::qsys::{QsysNode, parse_component, show_name};
use regfac::formats::tcp::op;

use super::Session;
use super::dirs::{describe, errno_text};
use crate::host::Host;
use crate::list::{civil, line1, member0, mode_string, secs, stat_line0, stat_line1};
use crate::names;
use crate::net::Net;
use crate::objects::{self, Kind};
use crate::reply::Reply;
use crate::xlate::{self, Rep};

/// What a listing found.
pub(super) struct Listed {
    /// Its lines.
    pub lines: Vec<String>,
    /// The first reply's text when it is not the usual (nothing found,
    /// a member not found in a file that is there).
    pub note: Option<String>,
}

impl Listed {
    fn of(lines: Vec<String>) -> Self {
        Self { lines, note: None }
    }
}

/// An object of a library.
struct LibObj {
    name: String,
    objtype: String,
    st: Option<Stat>,
}

/// A member of a file.
struct Mbr {
    name: String,
    st: Option<Stat>,
}

/// `ls` options (`-la`) dropped from a `LISTFMT 1` argument.
fn strip_options(arg: &str) -> String {
    arg.split_whitespace()
        .filter(|w| !(w.starts_with('-') && w.len() > 1))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The last component of a path.
fn base(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

impl<H: Host> Session<H> {
    fn offset(&self) -> i64 {
        self.host.utc_offset()
    }

    /// A path as a listing names it: relative to the current directory
    /// when it is under it.
    fn shown(&self, full: &str) -> String {
        let cwd = self.cwd.trim_end_matches('/');
        let under = if cwd.is_empty() { Some(full.trim_start_matches('/')) } else {
            full.strip_prefix(cwd).and_then(|r| r.strip_prefix('/'))
        };
        match under {
            Some(r) if !r.is_empty() => r.to_string(),
            _ => full.to_string(),
        }
    }

    /// The objects of a library, by name.
    fn lib_objects(&mut self, lib: &str) -> Result<Vec<LibObj>, ifs::Errno> {
        let dir = names::library_dir(lib);
        let mut out = Vec::new();
        for e in self.host.ifs().read_dir(&dir)? {
            let Ok((name, objtype)) = parse_component(&e.name) else {
                continue;
            };
            if objtype == "*LIB" {
                continue;
            }
            let st = self.host.ifs().stat(&format!("{dir}/{}", e.name), false).ok();
            out.push(LibObj { name, objtype, st });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name).then(a.objtype.cmp(&b.objtype)));
        Ok(out)
    }

    /// The members of a file, by name.
    fn file_members(&mut self, lib: &str, file: &str) -> Vec<Mbr> {
        let dir = objects::file_path(lib, file);
        let mut out = Vec::new();
        for e in self.host.ifs().read_dir(&dir).unwrap_or_default() {
            let Ok((name, t)) = parse_component(&e.name) else {
                continue;
            };
            if t != "*MBR" {
                continue;
            }
            let st = self.host.ifs().stat(&format!("{dir}/{}", e.name), false).ok();
            out.push(Mbr { name, st });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// A member's size as IBM shows it: two line-end bytes a record,
    /// whatever the file (the IFS counts them for source members only).
    fn member_size(&mut self, lib: &str, file: &str, st: &Stat) -> u64 {
        match self.host.file(lib, file) {
            Some(d) if !d.source && d.rec_len > 0 => st.size / d.rec_len as u64 * (d.rec_len as u64 + 2),
            _ => st.size,
        }
    }

    /// The `LISTFMT 1` line of a member.
    fn member_line1(&mut self, lib: &str, file: &str, st: &Stat, name: &str) -> String {
        let size = self.member_size(lib, file, st);
        line1(
            &mode_string(st.mode, false),
            1,
            &st.owner,
            "0",
            size,
            secs(st.mtime) + self.offset(),
            self.host.now() + self.offset(),
            name,
        )
    }

    /// The lines of the members of `lib/file` that `pick` takes, named
    /// `prefix` `FILE.MBR`.
    fn member_lines(&mut self, lib: &str, file: &str, pick: Option<&str>, prefix: &str, nlst: bool, out: &mut Vec<String>) -> usize {
        let owner = self
            .host
            .ifs()
            .stat(&objects::file_path(lib, file), false)
            .map(|s| s.owner)
            .unwrap_or_default();
        let mut n = 0;
        for m in self.file_members(lib, file) {
            if pick.is_some_and(|p| !names::matches(p, &m.name)) {
                continue;
            }
            n += 1;
            let name = format!("{prefix}{file}.{}", m.name);
            if nlst {
                out.push(name);
            } else if self.listfmt == 0 {
                out.push(member0(&owner, &name));
            } else if let Some(st) = &m.st {
                let l = self.member_line1(lib, file, st, &format!("{file}.{}", m.name));
                out.push(l);
            }
        }
        n
    }

    /// A file with its members (`LIB/FILE`, a library's listing).
    fn file_and_members(&mut self, lib: &str, o: &LibObj, prefix: &str, nlst: bool, out: &mut Vec<String>) {
        let shown_type = if o.objtype == "*SAVF" { "*FILE" } else { o.objtype.as_str() };
        if o.objtype != "*FILE" {
            // Other objects are in a full listing only.
            if !nlst && let Some(st) = &o.st {
                out.push(if self.listfmt == 0 {
                    stat_line0(st, shown_type, self.offset(), &format!("{prefix}{}", o.name))
                } else {
                    stat_line1(st, false, 1, self.offset(), self.host.now(), &o.name)
                });
            }
            return;
        }
        if !nlst && let Some(st) = &o.st {
            out.push(if self.listfmt == 0 {
                stat_line0(st, shown_type, self.offset(), &format!("{prefix}{}", o.name))
            } else {
                stat_line1(st, true, 1, self.offset(), self.host.now(), &o.name)
            });
        }
        if self.member_lines(lib, &o.name, None, prefix, nlst, out) == 0 && nlst {
            out.push(format!("{prefix}{}", o.name));
        }
    }

    /// A `NAMEFMT 0` listing.
    fn list0(&mut self, arg: &str, nlst: bool) -> Result<Listed, Reply> {
        let cur = self.curlib().unwrap_or_else(|| "QSYS".into());
        let arg = arg.trim();
        if arg.is_empty() {
            return self.list_library(&cur, &cur, nlst);
        }
        let n = names::parse_lib_name(arg).ok_or_else(|| Reply::new(501, format!("{arg} is not a name in format 0.")))?;
        match (&n.lib, &n.mbr) {
            (None, None) if names::generic(&n.file) => {
                let objs = self
                    .lib_objects(&cur)
                    .map_err(|_| Reply::new(550, format!("Library {cur} not found.")))?;
                let mut out = Vec::new();
                for o in objs.iter().filter(|o| names::matches(&n.file, &o.name)) {
                    self.file_and_members(&cur, o, "", nlst, &mut out);
                }
                if out.is_empty() {
                    return Ok(Listed {
                        lines: out,
                        note: Some(format!("No members (*ALL) in files {} of library {cur}.", n.file)),
                    });
                }
                Ok(Listed::of(out))
            }
            (None, None) => self.list_library(&n.file, &cur, nlst),
            (lib, mbr) => {
                let lib = lib.clone().unwrap_or_else(|| cur.clone());
                let prefix = if lib == cur { String::new() } else { format!("{lib}/") };
                let mut out = Vec::new();
                let file_obj = objects::file_path(&lib, &n.file);
                let is_file = objects::exists(self.host.ifs(), &file_obj);
                if !is_file {
                    if !objects::exists(self.host.ifs(), &names::library_dir(&lib)) {
                        return Err(Reply::new(550, format!("Library {lib} not found.")));
                    }
                    if mbr.is_some() && objects::exists(self.host.ifs(), &objects::savf_path(&lib, &n.file)) {
                        return Ok(Listed {
                            lines: out,
                            note: Some(format!("No members (*ALL) in file {} in library {lib}.", n.file)),
                        });
                    }
                    if names::generic(&n.file) {
                        let objs = self
                            .lib_objects(&lib)
                            .map_err(|_| Reply::new(550, format!("Library {lib} not found.")))?;
                        for o in objs.iter().filter(|o| o.objtype == "*FILE" && names::matches(&n.file, &o.name)) {
                            match mbr {
                                Some(m) => {
                                    self.member_lines(&lib, &o.name, Some(m), &prefix, nlst, &mut out);
                                }
                                None => self.file_and_members(&lib, o, &prefix, nlst, &mut out),
                            }
                        }
                        return Ok(Listed::of(out));
                    }
                    return Err(Reply::new(550, format!("File {} in library {lib} not found.", n.file)));
                }
                match mbr {
                    None => {
                        let st = self.host.ifs().stat(&file_obj, false).ok();
                        let o = LibObj {
                            name: n.file.clone(),
                            objtype: "*FILE".into(),
                            st,
                        };
                        self.file_and_members(&lib, &o, &prefix, nlst, &mut out);
                        Ok(Listed::of(out))
                    }
                    Some(m) => {
                        let found = self.member_lines(&lib, &n.file, Some(m), &prefix, nlst, &mut out);
                        if found == 0 {
                            let shown = if names::generic(m) && m == "*" { "*ALL" } else { m };
                            return Ok(Listed {
                                lines: out,
                                note: Some(format!("Member {shown} of file {} in library {lib} not found.", n.file)),
                            });
                        }
                        Ok(Listed::of(out))
                    }
                }
            }
        }
    }

    /// A library in `NAMEFMT 0`: its files with their members (and its
    /// other objects in a full listing).
    fn list_library(&mut self, lib: &str, cur: &str, nlst: bool) -> Result<Listed, Reply> {
        let objs = self
            .lib_objects(lib)
            .map_err(|_| Reply::new(550, format!("Library {lib} not found.")))?;
        let prefix = if lib == cur || self.listfmt == 1 && !nlst { String::new() } else { format!("{lib}/") };
        let mut out = Vec::new();
        for o in &objs {
            self.file_and_members(lib, o, &prefix, nlst, &mut out);
        }
        Ok(Listed::of(out))
    }

    /// One IFS object's line in `NAMEFMT 1`; `link` is `lstat`'s
    /// answer when it is a symbolic link.
    fn ifs_line(&mut self, full: &str, nlst: bool) -> Option<String> {
        let lst = self.host.ifs().stat(full, false).ok()?;
        let st = self.host.ifs().stat(full, true).unwrap_or_else(|_| lst.clone());
        let dir = objects::kind_of(full, &st).is_dir();
        let slash = if dir { "/" } else { "" };
        if nlst {
            return Some(format!("{}{slash}", self.shown(full)));
        }
        let off = self.offset();
        if self.listfmt == 0 {
            return Some(stat_line0(&st, &st.objtype, off, &format!("{}{slash}", self.shown(full))));
        }
        if lst.mode & S_IFMT == S_IFLNK {
            let target = self.host.ifs().readlink(full).unwrap_or_default();
            let mode = format!("l{}", &mode_string(lst.mode, false)[1..]);
            return Some(line1(
                &mode,
                lst.nlink.max(1),
                &lst.owner,
                "0",
                lst.size,
                secs(lst.mtime) + off,
                self.host.now() + off,
                &format!("{} -> {target}", base(full)),
            ));
        }
        Some(stat_line1(&st, dir, st.nlink.max(1), off, self.host.now(), base(full)))
    }

    /// A `NAMEFMT 1` listing of a path; its last part may be generic.
    #[allow(clippy::too_many_lines)] // one arm per kind of object
    fn list1(&mut self, arg: &str, nlst: bool) -> Result<Listed, Reply> {
        let arg = if self.listfmt == 1 { strip_options(arg) } else { arg.trim().to_string() };
        let typed = if arg.is_empty() { self.cwd.clone() } else { arg.clone() };
        let joined = if arg.is_empty() {
            self.cwd.clone()
        } else {
            names::join(&self.cwd, &arg, &self.home())
        };
        let (dir, pattern) = match joined.rsplit_once('/') {
            Some((d, last)) if names::generic(last) => (
                Self::canonical(if d.is_empty() { "/" } else { d }),
                Some(last.to_string()),
            ),
            _ => (Self::canonical(&joined), None),
        };
        let path = match &pattern {
            Some(p) => format!("{}/{p}", dir.trim_end_matches('/')),
            None => dir.clone(),
        };
        // A save file answers to its name as a file too.
        let dir = match names::qsys_node(&dir) {
            Some(QsysNode::Obj(l, o, t)) if t == "*FILE" && !objects::exists(self.host.ifs(), &dir) => {
                let savf = objects::savf_path(&l, &o);
                if objects::exists(self.host.ifs(), &savf) { savf } else { dir }
            }
            _ => dir,
        };
        // In a library, names need an extension the database knows.
        if names::in_qsys(&path) {
            let comps: Vec<&str> = path[names::QSYS_LIB.len()..].split('/').filter(|c| !c.is_empty()).collect();
            if comps.len() >= 2 {
                let last = comps[comps.len() - 1];
                let ext = last.rsplit_once('.').map(|(_, e)| e.to_ascii_uppercase());
                let ok = matches!(ext.as_deref(), Some("FILE" | "MBR" | "SAVF"));
                if !ok {
                    return Err(Reply::new(501, "Only .FILE and .MBR names can be listed in a library."));
                }
            }
        }
        let (k, st) = match objects::look(self.host.ifs(), &dir) {
            Ok(x) => x,
            Err(e) => {
                // A member not in a file that is there: an empty listing.
                if let Some(QsysNode::Mbr(l, f, m)) = names::qsys_node(&dir)
                    && objects::exists(self.host.ifs(), &objects::file_path(&l, &f))
                {
                    return Ok(Listed {
                        lines: Vec::new(),
                        note: Some(format!("Member {m} of file {f} in library {l} not found.")),
                    });
                }
                return Err(self.list_missing(&dir, &typed, e));
            }
        };
        let mut out = Vec::new();
        match k {
            Kind::Library(lib) => {
                for o in self.lib_objects(&lib).unwrap_or_default() {
                    let comp = format!("{}.{}", show_name(&o.name), o.objtype.trim_start_matches('*'));
                    if pattern.as_deref().is_some_and(|p| !names::matches(p, &comp)) {
                        continue;
                    }
                    let full = format!("{dir}/{comp}");
                    if o.objtype == "*FILE" && !nlst && self.listfmt == 0 {
                        if let Some(s) = &o.st {
                            out.push(stat_line0(s, "*FILE", self.offset(), &self.shown(&full)));
                        }
                        for m in self.file_members(&lib, &o.name) {
                            let mfull = format!("{full}/{}.MBR", show_name(&m.name));
                            out.push(member0(o.st.as_ref().map_or("", |s| s.owner.as_str()), &self.shown(&mfull)));
                        }
                    } else if let Some(l) = self.ifs_line(&full, nlst) {
                        out.push(l);
                    }
                }
            }
            Kind::File(lib, file) => {
                let members = self.file_members(&lib, &file);
                if !nlst && self.listfmt == 0 && pattern.is_none() {
                    out.push(stat_line0(&st, "*FILE", self.offset(), &self.shown(&dir)));
                }
                for m in members {
                    let mc = format!("{}.MBR", show_name(&m.name));
                    if pattern.as_deref().is_some_and(|p| !names::matches(p, &mc)) {
                        continue;
                    }
                    let mfull = format!("{dir}/{mc}");
                    if nlst {
                        out.push(self.shown(&mfull));
                    } else if self.listfmt == 0 {
                        out.push(member0(&st.owner, &self.shown(&mfull)));
                    } else if let Some(ms) = &m.st {
                        let l = self.member_line1(&lib, &file, ms, &mc);
                        out.push(l);
                    }
                }
            }
            Kind::Dir => {
                let mut ents = self
                    .host
                    .ifs()
                    .read_dir(&dir)
                    .map_err(|e| Reply::new(550, format!("{dir} cannot be listed: {}.", errno_text(e))))?;
                if names::in_qsys(&dir) {
                    ents.sort_by(|a, b| a.name.cmp(&b.name));
                }
                for e in ents {
                    if pattern.as_deref().is_some_and(|p| !names::matches(p, &e.name)) {
                        continue;
                    }
                    let full = if dir == "/" { format!("/{}", e.name) } else { format!("{dir}/{}", e.name) };
                    if let Some(l) = self.ifs_line(&full, nlst) {
                        out.push(l);
                    }
                }
                if out.is_empty() {
                    return Ok(Listed {
                        lines: out,
                        note: Some(format!("Directory {dir} is empty.")),
                    });
                }
            }
            Kind::Member(lib, file, _) => {
                if nlst {
                    out.push(self.shown(&path));
                } else if self.listfmt == 0 {
                    let owner = self
                        .host
                        .ifs()
                        .stat(&objects::file_path(&lib, &file), false)
                        .map_or(st.owner.clone(), |s| s.owner);
                    out.push(member0(&owner, &self.shown(&path)));
                } else {
                    let l = self.member_line1(&lib, &file, &st, base(&path));
                    out.push(l);
                }
            }
            Kind::Stream | Kind::Savf(..) | Kind::Object(..) => {
                if let Some(l) = self.ifs_line(&dir, nlst) {
                    out.push(l);
                }
            }
        }
        Ok(Listed::of(out))
    }

    /// Why a listing's path is not there.
    fn list_missing(&mut self, path: &str, typed: &str, e: ifs::Errno) -> Reply {
        match names::qsys_node(path) {
            Some(QsysNode::Lib(l)) => Reply::new(550, format!("Library {l} not found.")),
            Some(QsysNode::Obj(l, f, _)) => {
                if objects::exists(self.host.ifs(), &names::library_dir(&l)) {
                    Reply::new(550, format!("File {f} in library {l} not found."))
                } else {
                    Reply::new(550, format!("Library {l} not found."))
                }
            }
            Some(QsysNode::Mbr(..)) => self.missing(path, e),
            _ if e == ifs::Errno::ENOENT => Reply::new(550, format!("No such path: {typed}")),
            _ => Reply::new(550, format!("{typed} cannot be listed: {}.", errno_text(e))),
        }
    }

    /// The lines `LIST` or `NLST` of `arg` sends.
    pub(super) fn listing(&mut self, arg: &str, nlst: bool) -> Result<Listed, Reply> {
        if self.namefmt == 0 && !arg.trim().starts_with('/') {
            self.list0(arg, nlst)
        } else {
            self.list1(arg, nlst)
        }
    }

    pub(super) fn list(&mut self, arg: &str, nlst: bool, net: &mut dyn Net) -> Reply {
        let what = if arg.trim().is_empty() { self.cwd.clone() } else { arg.trim().to_string() };
        if !self.allowed(net, op::LIST, &what) {
            return Reply::new(550, format!("An exit program refused to list {what}."));
        }
        let listed = match self.listing(arg, nlst) {
            Ok(l) => l,
            Err(r) => return r,
        };
        let mut text = Vec::new();
        for l in &listed.lines {
            text.extend_from_slice(l.as_bytes());
            text.extend_from_slice(b"\r\n");
        }
        // A listing goes in the transfer's text form, its lines ended
        // by that form's carriage return and line feed.
        let to = match self.rep {
            Rep::Ebcdic => self.host.job_ccsid(),
            Rep::Ccsid(c) => c,
            _ => self.ascii(),
        };
        let mut data = xlate::convert(1208, to, &text);
        if xlate::is_ebcdic(to) {
            data = xlate::ebcdic_lines(&data);
        }
        let prelim = Reply::new(125, listed.note.unwrap_or_else(|| "Listing follows.".into()));
        self.send_data(net, &prelim, &data, Reply::new(250, "Listing complete."))
    }

    /// RFC 3659 facts of an object.
    fn facts(&mut self, path: &str, st: &Stat, kind: &Kind, name: &str) -> String {
        let t = if kind.is_dir() { "dir" } else { "file" };
        let c = civil(secs(st.mtime));
        let size = if kind.is_dir() {
            st.size
        } else {
            self.fetch(path).map_or(st.size, |d| d.len() as u64)
        };
        let perm = if kind.is_dir() { "elcmfd" } else { "rawdf" };
        format!(
            "type={t};size={size};modify={:04}{:02}{:02}{:02}{:02}{:02};perm={perm}; {name}",
            c.year, c.month, c.day, c.hour, c.minute, c.second
        )
    }

    fn fact_path(&mut self, arg: &str) -> Result<String, Reply> {
        if arg.trim().is_empty() {
            Ok(self.cwd.clone())
        } else {
            self.object_path(arg)
        }
    }

    pub(super) fn mlst(&mut self, arg: &str) -> Reply {
        let path = match self.fact_path(arg) {
            Ok(p) => p,
            Err(r) => return r,
        };
        match objects::look(self.host.ifs(), &path) {
            Ok((k, st)) => {
                let f = self.facts(&path, &st, &k, &path);
                Reply::lines(250, vec![format!("Facts of {path}:"), format!(" {f}"), "End.".into()])
            }
            Err(e) => self.list_missing(&path, &path, e),
        }
    }

    pub(super) fn mlsd(&mut self, arg: &str, net: &mut dyn Net) -> Reply {
        let path = match self.fact_path(arg) {
            Ok(p) => p,
            Err(r) => return r,
        };
        let (k, _) = match objects::look(self.host.ifs(), &path) {
            Ok(x) => x,
            Err(e) => return self.list_missing(&path, &path, e),
        };
        if !k.is_dir() {
            return Reply::new(501, format!("{path} is not a directory."));
        }
        let ents = match self.host.ifs().read_dir(&path) {
            Ok(e) => e,
            Err(e) => return Reply::new(550, format!("{path} cannot be listed: {}.", errno_text(e))),
        };
        let mut text = String::new();
        for e in ents {
            let full = if path == "/" { format!("/{}", e.name) } else { format!("{path}/{}", e.name) };
            if let Ok((k, st)) = objects::look(self.host.ifs(), &full) {
                let f = self.facts(&full, &st, &k, &e.name);
                text.push_str(&f);
                text.push_str("\r\n");
            }
        }
        let data = xlate::convert(1208, self.ascii(), text.as_bytes());
        self.send_data(net, &Reply::new(150, "Listing follows."), &data, Reply::new(226, "Listing complete."))
    }

    pub(super) fn size(&mut self, arg: &str) -> Reply {
        let path = match self.object_path(arg) {
            Ok(p) => p,
            Err(r) => return r,
        };
        match objects::look(self.host.ifs(), &path) {
            Ok((k, _)) if k.is_dir() && !matches!(k, Kind::File(..)) => {
                Reply::new(550, format!("{} is a directory or library.", describe(&path)))
            }
            Ok(_) => match self.fetch(&path) {
                Ok(d) => Reply::new(213, d.len().to_string()),
                Err(r) => r,
            },
            Err(e) => self.missing(&path, e),
        }
    }

    pub(super) fn mdtm(&mut self, arg: &str) -> Reply {
        let path = match self.object_path(arg) {
            Ok(p) => p,
            Err(r) => return r,
        };
        match objects::look(self.host.ifs(), &path) {
            Ok((_, st)) => {
                let c = civil(secs(st.mtime));
                Reply::new(
                    213,
                    format!("{:04}{:02}{:02}{:02}{:02}{:02}", c.year, c.month, c.day, c.hour, c.minute, c.second),
                )
            }
            Err(e) => self.missing(&path, e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_dropped() {
        assert_eq!(strip_options("-la"), "");
        assert_eq!(strip_options("-a /home/x"), "/home/x");
        assert_eq!(strip_options("x"), "x");
        assert_eq!(base("/a/b"), "b");
    }
}
