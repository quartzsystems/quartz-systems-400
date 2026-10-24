// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Directory streams: `opendir()`, `readdir()`, `rewinddir()`,
//! `closedir()`. A stream reads the directory when it opens and again
//! at `rewinddir()`; mount points show as entries of the directory
//! they are in.

use super::{Ctx, Ifs};
use crate::auth::need::Op;
use crate::backend::{Backend, Kind};
use crate::errno::{Errno, Result};
use crate::path::fold;
use crate::resolve::{Node, NodeType, Res, Sys};
use crate::vfs::UDFS_BASE;
use crate::vfs::dev::DEVICES;
use crate::vfs::qsys::{self, QsysNode};

/// One entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEnt {
    /// The name.
    pub name: String,
    /// File number (`d_fileno`).
    pub ino: u64,
    /// What it is.
    pub ntype: NodeType,
}

/// An open directory stream.
#[derive(Debug, Clone)]
pub(crate) struct DirStream {
    res: Res,
    ents: Vec<DirEnt>,
    pos: usize,
}

fn ntype_of(k: Kind) -> NodeType {
    match k {
        Kind::File => NodeType::File,
        Kind::Dir => NodeType::Dir,
        Kind::Symlink => NodeType::Symlink,
        Kind::Fifo => NodeType::Fifo,
        Kind::Socket => NodeType::Socket,
        Kind::CharDev => NodeType::Chr,
    }
}

impl<B: Backend> Ifs<B> {
    /// The entries of a directory, with `.` and `..`.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn list(&self, sys: &mut Sys<'_, B>, r: &Res) -> Result<Vec<DirEnt>> {
        let mut out = vec![
            DirEnt {
                name: ".".into(),
                ino: r.node.ino(),
                ntype: NodeType::Dir,
            },
            DirEnt {
                name: "..".into(),
                ino: {
                    let mut up = r.comps.clone();
                    up.pop();
                    sys.locate(&up).map_or(1, |n| n.ino())
                },
                ntype: NodeType::Dir,
            },
        ];
        let mut ents: Vec<DirEnt> = match &r.node {
            Node::Back { rel, .. } => self
                .b
                .readdir(rel)?
                .into_iter()
                .map(|e| DirEnt {
                    name: e.name,
                    ino: e.ino,
                    ntype: ntype_of(e.kind),
                })
                .collect(),
            Node::DevDir { fs } => {
                let mut v: Vec<DirEnt> = DEVICES
                    .iter()
                    .map(|(n, d)| DirEnt {
                        name: (*n).to_string(),
                        ino: d.ino(),
                        ntype: NodeType::Chr,
                    })
                    .collect();
                if *fs == crate::vfs::ids::DEV {
                    for a in sys.asps()? {
                        v.push(DirEnt {
                            ino: Node::AspDir { asp: a.clone() }.ino(),
                            name: a,
                            ntype: NodeType::Dir,
                        });
                    }
                }
                v
            }
            Node::AspDir { asp } => self
                .b
                .readdir(&crate::backend::join(UDFS_BASE, asp))
                .unwrap_or_default()
                .into_iter()
                .filter(|e| {
                    std::path::Path::new(&e.name)
                        .extension()
                        .is_some_and(|x| x.eq_ignore_ascii_case("udfs"))
                })
                .map(|e| DirEnt {
                    name: e.name,
                    ino: e.ino,
                    ntype: NodeType::Blk,
                })
                .collect(),
            Node::Qsys(n, _) => {
                let objs = |lib: &str| -> Result<Vec<DirEnt>> {
                    Ok(self
                        .qsys
                        .objects(lib)?
                        .into_iter()
                        .map(|e| {
                            let node = QsysNode::Obj(lib.into(), e.name.clone(), e.objtype.clone());
                            let ntype = Node::Qsys(node.clone(), e.clone()).ntype();
                            DirEnt {
                                name: format!(
                                    "{}.{}",
                                    qsys::show_name(&e.name),
                                    qsys::ext_of_type(&e.objtype)
                                ),
                                ino: Node::Qsys(node, e).ino(),
                                ntype,
                            }
                        })
                        .collect())
                };
                match n {
                    QsysNode::Root => {
                        let mut v: Vec<DirEnt> = self
                            .qsys
                            .libraries()?
                            .into_iter()
                            .map(|e| {
                                let node = QsysNode::Lib(e.name.clone());
                                DirEnt {
                                    name: format!("{}.LIB", qsys::show_name(&e.name)),
                                    ino: Node::Qsys(node, e).ino(),
                                    ntype: NodeType::Dir,
                                }
                            })
                            .collect();
                        v.extend(objs("QSYS")?);
                        v
                    }
                    QsysNode::Lib(l) => objs(l)?,
                    QsysNode::Obj(l, file, t) if t == "*FILE" => self
                        .qsys
                        .members(l, file)?
                        .into_iter()
                        .map(|e| {
                            let node = QsysNode::Mbr(l.clone(), file.clone(), e.name.clone());
                            DirEnt {
                                name: format!("{}.MBR", qsys::show_name(&e.name)),
                                ino: Node::Qsys(node, e).ino(),
                                ntype: NodeType::File,
                            }
                        })
                        .collect(),
                    _ => return Err(Errno::ENOTDIR),
                }
            }
            Node::OptRoot => self
                .opt
                .volumes()
                .into_iter()
                .map(|v| DirEnt {
                    ino: 0,
                    name: v,
                    ntype: NodeType::Dir,
                })
                .collect(),
            Node::Opt { vol, path, .. } => self
                .opt
                .readdir(vol, path)?
                .into_iter()
                .map(|e| DirEnt {
                    name: e.name,
                    ino: 0,
                    ntype: ntype_of(e.kind),
                })
                .collect(),
            Node::Unsup { .. } => Vec::new(),
            _ => return Err(Errno::ENOTDIR),
        };
        // Mount points replace whatever the directory itself holds there.
        let here = sys.mounts.mount_of(&r.comps).clone();
        for m in sys.mounts.children(&r.comps) {
            let name = m.at.last().expect("mount point").clone();
            ents.retain(|e| {
                if here.case_sensitive {
                    e.name != name
                } else {
                    !fold::same(&e.name, &name)
                }
            });
            ents.push(DirEnt {
                name,
                ino: 1,
                ntype: NodeType::Dir,
            });
        }
        ents.sort_by(|a, b| a.name.cmp(&b.name));
        out.extend(ents);
        Ok(out)
    }

    /// `opendir()`: needs `*R` on the directory. Returns a handle.
    pub fn opendir(&self, cx: &Ctx<'_>, p: &str) -> Result<u64> {
        let mut st = self.lock();
        let cwd = Self::cwd_of(&mut st, cx.owner);
        let (mut sys, rest) = self.split(&mut st);
        let r = sys.resolve(&cwd, p, true, Some(cx))?;
        if !r.node.is_dir() {
            return Err(Errno::ENOTDIR);
        }
        if let Node::Unsup { .. } = r.node {
            return Err(Errno::ENOTSUP);
        }
        let m = sys.check(&r.node, &r.path(), Some(cx), Op::Readdir)?;
        let ents = self.list(&mut sys, &r)?;
        cx.audit_access(&m, &r.node, &r.path(), Op::Readdir);
        let os = self.owner_state(rest, cx.owner);
        let h = os.next_dir;
        os.next_dir += 1;
        os.dirs.insert(
            h,
            DirStream {
                res: r,
                ents,
                pos: 0,
            },
        );
        Ok(h)
    }

    /// `readdir()`: the next entry, `None` at the end.
    pub fn readdir(&self, cx: &Ctx<'_>, h: u64) -> Result<Option<DirEnt>> {
        let mut st = self.lock();
        let ds = st
            .rest
            .owners
            .get_mut(&cx.owner)
            .and_then(|o| o.dirs.get_mut(&h))
            .ok_or(Errno::EBADF)?;
        let e = ds.ents.get(ds.pos).cloned();
        if e.is_some() {
            ds.pos += 1;
        }
        Ok(e)
    }

    /// `rewinddir()`: read the directory again.
    pub fn rewinddir(&self, cx: &Ctx<'_>, h: u64) -> Result<()> {
        let mut st = self.lock();
        let res = st
            .rest
            .owners
            .get(&cx.owner)
            .and_then(|o| o.dirs.get(&h))
            .ok_or(Errno::EBADF)?
            .res
            .clone();
        let (mut sys, rest) = self.split(&mut st);
        let ents = match sys.locate(&res.comps) {
            Ok(node) => self.list(
                &mut sys,
                &Res {
                    comps: res.comps.clone(),
                    node,
                },
            )?,
            Err(_) => Vec::new(),
        };
        let ds = rest
            .owners
            .get_mut(&cx.owner)
            .and_then(|o| o.dirs.get_mut(&h))
            .ok_or(Errno::EBADF)?;
        ds.ents = ents;
        ds.pos = 0;
        Ok(())
    }

    /// `closedir()`.
    pub fn closedir(&self, cx: &Ctx<'_>, h: u64) -> Result<()> {
        let mut st = self.lock();
        st.rest
            .owners
            .get_mut(&cx.owner)
            .and_then(|o| o.dirs.remove(&h))
            .map(|_| ())
            .ok_or(Errno::EBADF)
    }

    /// Every entry of a directory (without `.` and `..`), for commands.
    pub fn read_dir(&self, cx: &Ctx<'_>, p: &str) -> Result<Vec<DirEnt>> {
        let h = self.opendir(cx, p)?;
        let mut v = Vec::new();
        while let Some(e) = self.readdir(cx, h)? {
            if e.name != "." && e.name != ".." {
                v.push(e);
            }
        }
        self.closedir(cx, h)?;
        Ok(v)
    }
}
