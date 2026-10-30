// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Names and directories: how an argument becomes a path in either
//! naming format, `PWD`, `CWD`, `CDUP`, `MKD`, `RMD`, `DELE`, `RNFR`,
//! `RNTO`.

use ifs::ops::RenameMode;
use ifs::vfs::qsys::QsysNode;
use regfac::formats::tcp::op;

use super::Session;
use crate::host::Host;
use crate::names::{self, LibName};
use crate::net::Net;
use crate::objects::{self, Kind};
use crate::reply::Reply;

pub(super) use crate::store::{describe, errno_text};

impl<H: Host> Session<H> {
    /// A `NAMEFMT 1` path made canonical: `/QSYS.LIB` names upper
    /// case.
    pub(super) fn canonical(path: &str) -> String {
        match names::qsys_node(path) {
            Some(n) => names::qsys_path(&n),
            None => path.to_string(),
        }
    }

    /// The path an object command's argument names.
    pub(super) fn object_path(&mut self, arg: &str) -> Result<String, Reply> {
        let arg = arg.trim();
        if arg.is_empty() {
            return Err(Reply::new(501, "A name is needed."));
        }
        if self.namefmt == 1 || arg.starts_with('/') {
            return Ok(Self::canonical(&names::join(&self.cwd, arg, &self.home())));
        }
        let n = names::parse_lib_name(arg).ok_or_else(|| Reply::new(501, format!("{arg} is not a name in format 0.")))?;
        Ok(self.lib_name_path(&n))
    }

    /// The path `DELE`, `RNFR` and `RNTO` name: in `NAMEFMT 0` a name
    /// without a member is the file itself.
    pub(super) fn file_or_member_path(&mut self, arg: &str) -> Result<String, Reply> {
        let arg = arg.trim();
        if self.namefmt == 1 || arg.starts_with('/') {
            return self.object_path(arg);
        }
        let n = names::parse_lib_name(arg).ok_or_else(|| Reply::new(501, format!("{arg} is not a name in format 0.")))?;
        if n.mbr.is_some() {
            return Ok(self.lib_name_path(&n));
        }
        let lib = n.lib.clone().or_else(|| self.curlib()).unwrap_or_else(|| "QGPL".into());
        let savf = objects::savf_path(&lib, &n.file);
        if objects::exists(self.host.ifs(), &savf) {
            return Ok(savf);
        }
        Ok(objects::file_path(&lib, &n.file))
    }

    /// A `NAMEFMT 0` name's path: the current library when none is
    /// given, the file's name as the member when none is given (a save
    /// file of that name is the save file).
    pub(super) fn lib_name_path(&mut self, n: &LibName) -> String {
        let lib = n.lib.clone().or_else(|| self.curlib()).unwrap_or_else(|| "QGPL".into());
        if let Some(m) = &n.mbr {
            objects::member_path(&lib, &n.file, m)
        } else {
            let savf = objects::savf_path(&lib, &n.file);
            if objects::exists(self.host.ifs(), &savf) {
                savf
            } else {
                objects::member_path(&lib, &n.file, &n.file)
            }
        }
    }

    /// `"path" is current directory` (or library), as clients parse it.
    fn place_text(&self) -> String {
        if self.namefmt == 0 {
            let lib = self.curlib().unwrap_or_else(|| "QSYS".into());
            return format!("\"{lib}\" is current library.");
        }
        let what = if names::library_of(&self.cwd).is_some() {
            "library"
        } else {
            "directory"
        };
        format!("\"{}\" is current {what}.", self.cwd)
    }

    pub(super) fn pwd(&self) -> Reply {
        Reply::new(257, self.place_text())
    }

    /// Make `path` the current directory when it can be one.
    fn change_to(&mut self, path: String, net: &mut dyn Net) -> Reply {
        let lib0 = self.namefmt == 0;
        let not_changed = |why: &str| {
            Reply::lines(
                550,
                vec![
                    why.to_string(),
                    if lib0 {
                        "The current library stays as it was.".to_string()
                    } else {
                        "The current directory stays as it was.".to_string()
                    },
                ],
            )
        };
        if !self.allowed(net, op::SET_DIR, &path) {
            return not_changed("An exit program refused the change.");
        }
        match objects::look(self.host.ifs(), &path) {
            Ok((k, _)) if k.is_dir() => {
                if lib0 && !matches!(k, Kind::Library(_)) && path != names::QSYS_LIB {
                    // Format 0 lives in libraries.
                    self.namefmt = 1;
                }
                self.cwd = path;
                if self.namefmt == 1 {
                    self.lib_mode = false;
                }
                Reply::new(250, self.place_text())
            }
            Ok(_) => not_changed(&format!("{} is not a directory or library.", describe(&path))),
            Err(e) => not_changed(&format!(
                "{} cannot be used: {}.",
                if lib0 { describe(&path) } else { path.clone() },
                errno_text(e)
            )),
        }
    }

    /// A `NAMEFMT 1` directory argument as a path. In `/QSYS.LIB` a
    /// relative library name is a library beside the current one.
    fn dir_path(&self, arg: &str) -> String {
        if !arg.starts_with('/')
            && names::in_qsys(&self.cwd)
            && arg
                .split('/')
                .next()
                .is_some_and(|c| c.to_ascii_uppercase().ends_with(".LIB"))
        {
            return Self::canonical(&names::join(names::QSYS_LIB, arg, &self.home()));
        }
        Self::canonical(&names::join(&self.cwd, arg, &self.home()))
    }

    pub(super) fn cwd_cmd(&mut self, arg: &str, net: &mut dyn Net) -> Reply {
        let arg = arg.trim();
        if arg.is_empty() {
            return Reply::new(250, self.place_text());
        }
        if self.namefmt == 0 && !arg.starts_with('/') {
            return self.cwd_library(arg, net);
        }
        if self.namefmt == 0 {
            if self.lib_mode {
                return Reply::new(501, "In name format 0 a name is LIBRARY/FILE.MEMBER.");
            }
            let path = self.dir_path(arg);
            self.namefmt = 1;
            let r = self.change_to(path, net);
            if r.failed() {
                self.namefmt = 0;
                return r;
            }
            let mut lines = vec!["Name format is now 1.".to_string()];
            lines.extend(r.lines);
            return Reply::lines(250, lines);
        }
        let path = self.dir_path(arg);
        if let Some(QsysNode::Mbr(..)) = names::qsys_node(&path) {
            return Reply::new(550, "A member cannot be the current library; nothing changed.");
        }
        self.change_to(path, net)
    }

    /// `CWD LIB` in `NAMEFMT 0`.
    fn cwd_library(&mut self, arg: &str, net: &mut dyn Net) -> Reply {
        let lib = arg.to_ascii_uppercase();
        if let Some(c) = lib.chars().next().filter(char::is_ascii_digit) {
            return Reply::new(501, format!("A name cannot start with ({c})."));
        }
        if let Some(c) = lib
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '$' | '#' | '@' | '_' | '.')))
        {
            return Reply::new(501, format!("A library name cannot hold ({c})."));
        }
        if lib.chars().count() > 10 {
            return Reply::new(501, format!("{lib} is longer than the ten characters of a name."));
        }
        if lib.contains('.') {
            return Reply::lines(
                550,
                vec![
                    format!("Library {lib} cannot be found or used."),
                    "The current library stays as it was.".into(),
                ],
            );
        }
        let r = self.change_to(names::library_dir(&lib), net);
        if !r.failed() {
            self.lib_mode = true;
        }
        r
    }

    pub(super) fn cdup(&mut self, net: &mut dyn Net) -> Reply {
        if self.namefmt == 0 {
            let r = self.change_to(names::QSYS_LIB.to_string(), net);
            if !r.failed() {
                self.lib_mode = true;
            }
            return r;
        }
        let up = names::normalize(&format!("{}/..", self.cwd));
        self.change_to(up, net)
    }

    pub(super) fn mkd(&mut self, arg: &str, net: &mut dyn Net) -> Reply {
        let path = if self.namefmt == 0 && !arg.trim().starts_with('/') {
            match names::parse_lib_name(arg) {
                Some(n) if n.lib.is_none() && n.mbr.is_none() => names::library_dir(&n.file),
                _ => return Reply::new(501, format!("{} is not a library name.", arg.trim())),
            }
        } else {
            match self.object_path(arg) {
                Ok(p) => p,
                Err(r) => return r,
            }
        };
        if !self.allowed(net, op::CREATE_DIR, &path) {
            return Reply::new(550, format!("An exit program refused to create {path}."));
        }
        if objects::exists(self.host.ifs(), &path) {
            return Reply::new(550, format!("{} exists already; nothing was created.", describe(&path)));
        }
        let made = match names::qsys_node(&path) {
            Some(QsysNode::Lib(l)) => self.host.command(&format!("CRTLIB LIB({l})")).map_err(|e| e.text),
            Some(_) => return Reply::new(501, "Only a library can be made in /QSYS.LIB."),
            None => self.host.ifs().mkdir(&path, 0o777).map_err(errno_text),
        };
        match made {
            Ok(()) => Reply::new(250, format!("Created {}.", describe(&path))),
            Err(e) => Reply::new(550, format!("{} not created: {e}.", describe(&path))),
        }
    }

    pub(super) fn rmd(&mut self, arg: &str, net: &mut dyn Net) -> Reply {
        let path = if self.namefmt == 0 && !arg.trim().starts_with('/') {
            match names::parse_lib_name(arg) {
                Some(n) if n.lib.is_none() && n.mbr.is_none() => names::library_dir(&n.file),
                _ => return Reply::new(501, format!("{} is not a library name.", arg.trim())),
            }
        } else {
            match self.object_path(arg) {
                Ok(p) => p,
                Err(r) => return r,
            }
        };
        if !self.allowed(net, op::DELETE_DIR, &path) {
            return Reply::new(550, format!("An exit program refused to delete {path}."));
        }
        let gone = match names::qsys_node(&path) {
            Some(QsysNode::Lib(l)) => self.host.command(&format!("DLTLIB LIB({l})")).map_err(|e| e.text),
            Some(QsysNode::Obj(l, f, t)) if t == "*FILE" => {
                self.host.command(&format!("DLTF FILE({l}/{f})")).map_err(|e| e.text)
            }
            Some(_) => Err("it is not a directory".into()),
            None => match objects::look(self.host.ifs(), &path) {
                Ok((Kind::Dir, _)) => self.host.ifs().rmdir(&path).map_err(errno_text),
                Ok(_) => Err("it is not a directory".into()),
                Err(e) => Err(errno_text(e)),
            },
        };
        match gone {
            Ok(()) => Reply::new(250, format!("Deleted {}.", describe(&path))),
            Err(e) => Reply::new(550, format!("{} not deleted: {e}.", describe(&path))),
        }
    }

    pub(super) fn dele(&mut self, arg: &str, net: &mut dyn Net) -> Reply {
        let path = match self.file_or_member_path(arg) {
            Ok(p) => p,
            Err(r) => return r,
        };
        if !self.allowed(net, op::DELETE_FILE, &path) {
            return Reply::new(550, format!("An exit program refused to delete {}.", describe(&path)));
        }
        let gone = match objects::look(self.host.ifs(), &path) {
            Ok((Kind::File(l, f) | Kind::Savf(l, f), _)) => {
                self.host.command(&format!("DLTF FILE({l}/{f})")).map_err(|e| e.text)
            }
            Ok((k, _)) if k.is_dir() => Err("it is a directory".to_string()),
            Ok(_) => self.host.ifs().unlink(&path).map_err(errno_text),
            Err(e) => Err(errno_text(e)),
        };
        match gone {
            Ok(()) => Reply::new(250, format!("Deleted {}.", describe(&path))),
            Err(e) => Reply::new(550, format!("{} not deleted: {e}.", describe(&path))),
        }
    }

    pub(super) fn rnfr_cmd(&mut self, arg: &str, net: &mut dyn Net) -> Reply {
        let path = match self.file_or_member_path(arg) {
            Ok(p) => p,
            Err(r) => return r,
        };
        if !self.allowed(net, op::RENAME, &path) {
            return Reply::new(550, format!("An exit program refused to rename {}.", describe(&path)));
        }
        if let Err(e) = self.host.ifs().stat(&path, false) {
            return Reply::new(550, format!("{} cannot be renamed: {}.", describe(&path), errno_text(e)));
        }
        let shown = describe(&path);
        self.rnfr = Some(path);
        Reply::new(350, format!("Send RNTO with the new name of {shown}."))
    }

    pub(super) fn rnto(&mut self, arg: &str, from: Option<String>, net: &mut dyn Net) -> Reply {
        let _ = net;
        let Some(from) = from else {
            return Reply::new(503, "Send RNFR first.");
        };
        let to = match (names::qsys_node(&from), self.namefmt, arg.trim().starts_with('/')) {
            // In format 0 a bare new name stays in the same file.
            (Some(QsysNode::Mbr(l, f, _)), 0, false) => match names::parse_lib_name(arg) {
                Some(n) if n.lib.is_none() && n.mbr.is_none() => objects::member_path(&l, &f, &n.file),
                Some(n) => {
                    let lib = n.lib.clone().unwrap_or(l);
                    objects::member_path(&lib, &n.file, n.mbr.as_deref().unwrap_or(&n.file))
                }
                None => return Reply::new(550, format!("{} is not a name in format 0.", arg.trim())),
            },
            _ => match self.file_or_member_path(arg) {
                Ok(p) => p,
                Err(r) => return r,
            },
        };
        let done = match (names::qsys_node(&from), names::qsys_node(&to)) {
            (Some(QsysNode::Mbr(l, f, m)), Some(QsysNode::Mbr(l2, f2, m2))) => {
                if l != l2 {
                    Err("a member can only be renamed within its library".into())
                } else if f == f2 {
                    self.host
                        .command(&format!("RNMM FILE({l}/{f}) MBR({m}) NEWMBR({m2})"))
                        .map_err(|e| e.text)
                } else {
                    self.move_member(&l, &f, &m, &f2, &m2)
                }
            }
            (Some(QsysNode::Obj(l, o, t)), Some(n2)) => {
                let new = match n2 {
                    QsysNode::Obj(l2, o2, _) if l2 == l => Some(o2),
                    QsysNode::Mbr(l2, f2, _) if l2 == l => Some(f2),
                    _ => None,
                };
                match new {
                    Some(o2) => self
                        .host
                        .command(&format!("RNMOBJ OBJ({l}/{o}) OBJTYPE({t}) NEWOBJ({o2})"))
                        .map_err(|e| e.text),
                    None => Err("an object can only be renamed within its library".into()),
                }
            }
            (Some(QsysNode::Mbr(..) | QsysNode::Obj(..)), None) => {
                Err("a library object cannot be moved out of /QSYS.LIB".into())
            }
            _ => self.host.ifs().rename(&from, &to, RenameMode::Keep).map_err(errno_text),
        };
        match done {
            Ok(()) => Reply::new(250, format!("Renamed {} to {}.", describe(&from), describe(&to))),
            Err(e) => Reply::new(550, format!("{} not renamed: {e}.", describe(&from))),
        }
    }
}
