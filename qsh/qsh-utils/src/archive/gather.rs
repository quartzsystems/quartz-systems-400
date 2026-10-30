// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Gathering files to archive: walking each operand and turning what
//! is found into members, a second name of a file becoming a hard
//! link to the first.

use qsh::sys::{FileType, Stat, SysError, UtilCtx};

use super::member::{Kind, Member};
use crate::common::io::read_input;
use crate::common::shown;
use crate::common::walk::{Control, Visit, Walker};

/// How to walk.
#[derive(Clone, Copy, Debug, Default)]
pub struct Gather {
    /// Follow symbolic links named as operands (`-H`).
    pub follow_root: bool,
    /// Follow every symbolic link (`-L`, `tar h`).
    pub follow_all: bool,
    /// Do not descend into directories (`-d`).
    pub no_descend: bool,
    /// Stay on the operand's file system (`-X`).
    pub one_fs: bool,
}

/// A path found on a walk.
#[derive(Clone, Debug)]
pub struct Found {
    /// The name as the operand spells it.
    pub name: String,
    /// The absolute path.
    pub abs: String,
    /// Its status.
    pub st: Stat,
}

impl Gather {
    /// Walk an operand in order, directories before their contents.
    /// `f` gets each path or a failure; `false` from it stops.
    pub fn walk(
        self,
        cx: &UtilCtx<'_>,
        op: &str,
        f: &mut dyn FnMut(Result<Found, (String, SysError)>) -> bool,
    ) -> bool {
        let abs = cx.path(op);
        let w = Walker {
            follow_root: self.follow_root,
            follow_all: self.follow_all,
            one_fs: self.one_fs,
        };
        let no_descend = self.no_descend;
        w.walk(cx.sys, &abs, &mut |v| match v {
            Visit::Pre(e) => {
                let name = shown(op, &abs, &e.path);
                let go = f(Ok(Found {
                    name,
                    abs: e.path.clone(),
                    st: e.st,
                }));
                if !go {
                    Control::Stop
                } else if no_descend {
                    Control::Prune
                } else {
                    Control::Continue
                }
            }
            Visit::Post(_) => Control::Continue,
            Visit::Fail(p, err) => {
                if f(Err((shown(op, &abs, p), err))) {
                    Control::Continue
                } else {
                    Control::Stop
                }
            }
        })
    }
}

/// The member name of a path: no final slash, `/` kept.
pub fn member_name(name: &str) -> String {
    let t = name.trim_end_matches('/');
    if t.is_empty() && name.starts_with('/') {
        "/".to_string()
    } else {
        t.to_string()
    }
}

/// Files already archived by file ID, for hard links.
#[derive(Default)]
pub struct Links(Vec<((u64, u64), String)>);

impl Links {
    /// The member name first archived for a file, recording `name` when
    /// it is the first.
    pub fn first(&mut self, st: &Stat, name: &str) -> Option<String> {
        if st.ftype != FileType::File || st.nlink < 2 {
            return None;
        }
        let key = (st.dev, st.ino);
        if let Some((_, n)) = self.0.iter().find(|(k, _)| *k == key) {
            return Some(n.clone());
        }
        self.0.push((key, name.to_string()));
        None
    }
}

/// Turn a found path into a member named `name`.
pub fn member(
    cx: &UtilCtx<'_>,
    found: &Found,
    name: &str,
    links: &mut Links,
) -> Result<Member, SysError> {
    let st = &found.st;
    let kind = match st.ftype {
        FileType::File | FileType::Other => Kind::File,
        FileType::Dir => Kind::Dir,
        FileType::Symlink => Kind::Symlink,
        FileType::Fifo => Kind::Fifo,
        FileType::Char => Kind::Char,
        FileType::Block => Kind::Block,
        FileType::Socket => return Err(qsh::sys::Errno::EOPNOTSUPP.into()),
    };
    let mut m = Member::new(name, kind);
    m.mode = st.mode & 0o7777;
    m.uid = st.uid;
    m.gid = st.gid;
    m.uname = cx.sys.user_name(st.uid).unwrap_or_default();
    m.gname = cx.sys.group_name(st.gid).unwrap_or_default();
    m.mtime = st.mtime;
    m.dev = st.dev;
    m.ino = st.ino;
    m.nlink = st.nlink;
    match kind {
        Kind::File => {
            m.data = read_input(cx, &found.abs)?;
            if let Some(first) = links.first(st, name) {
                m.kind = Kind::Hardlink;
                m.link = first;
            }
        }
        Kind::Symlink => m.link = cx.sys.readlink(&found.abs)?,
        _ => {}
    }
    Ok(m)
}
