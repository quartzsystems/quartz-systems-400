// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SAV` and `RST`: the walk and the per-object descriptor a later phase
//! writes to media. A save is a sequence of records: one descriptor per
//! link (path, type, the metadata record as stored, times, size, the
//! link target or the first path of an inode already saved), each
//! stream file's data following its descriptor. No media code lives
//! here; `docs/IFS.md` has the layout.

use crate::backend::{Backend, Nanos};
use crate::errno::{Errno, Result};
use crate::meta::{Flags, codec};
use crate::ops::{Ctx, Ifs, Stat, Visit, WalkOpts};

/// The descriptor's magic.
pub const MAGIC: &[u8; 4] = b"QSVD";
/// The descriptor version.
pub const VERSION: u8 = 1;

/// One saved link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    /// The path saved.
    pub path: String,
    /// `*STMF`, `*DIR`, `*SYMLNK`, `*FIFO`, `*SOCKET`.
    pub objtype: String,
    /// The metadata record, as `meta::codec` encodes it.
    pub meta: Vec<u8>,
    /// Last access.
    pub atime: Nanos,
    /// Last data change.
    pub mtime: Nanos,
    /// Data bytes following the descriptor.
    pub size: u64,
    /// For a symbolic link: its text.
    pub link_target: Option<String>,
    /// For a second link to an inode already saved: the first path.
    pub hard_link_of: Option<String>,
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&u32::try_from(s.len()).expect("under 4G").to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn get_str(raw: &[u8], at: &mut usize) -> Result<String> {
    let n = u32::from_le_bytes(
        raw.get(*at..*at + 4)
            .ok_or(Errno::EINVAL)?
            .try_into()
            .expect("4"),
    ) as usize;
    *at += 4;
    let s = raw.get(*at..*at + n).ok_or(Errno::EINVAL)?;
    *at += n;
    String::from_utf8(s.to_vec()).map_err(|_| Errno::EILSEQ)
}

impl Descriptor {
    /// Encode: magic, version, then path, type, record (length-prefixed),
    /// times and size (`i64`, `i64`, `u64` little-endian), and the two
    /// optional strings (a presence byte, then the string).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.push(VERSION);
        put_str(&mut out, &self.path);
        put_str(&mut out, &self.objtype);
        out.extend_from_slice(&u32::try_from(self.meta.len()).expect("small").to_le_bytes());
        out.extend_from_slice(&self.meta);
        out.extend_from_slice(&self.atime.to_le_bytes());
        out.extend_from_slice(&self.mtime.to_le_bytes());
        out.extend_from_slice(&self.size.to_le_bytes());
        for o in [&self.link_target, &self.hard_link_of] {
            match o {
                Some(s) => {
                    out.push(1);
                    put_str(&mut out, s);
                }
                None => out.push(0),
            }
        }
        out
    }

    /// Decode.
    pub fn decode(raw: &[u8]) -> Result<Self> {
        if raw.len() < 5 || &raw[..4] != MAGIC || raw[4] > VERSION {
            return Err(Errno::EINVAL);
        }
        let mut at = 5;
        let path = get_str(raw, &mut at)?;
        let objtype = get_str(raw, &mut at)?;
        let n = u32::from_le_bytes(
            raw.get(at..at + 4)
                .ok_or(Errno::EINVAL)?
                .try_into()
                .expect("4"),
        ) as usize;
        at += 4;
        let meta = raw.get(at..at + n).ok_or(Errno::EINVAL)?.to_vec();
        at += n;
        let mut num = |len: usize| -> Result<[u8; 8]> {
            let v: [u8; 8] = raw
                .get(at..at + len)
                .ok_or(Errno::EINVAL)?
                .try_into()
                .map_err(|_| Errno::EINVAL)?;
            at += len;
            Ok(v)
        };
        let atime = i64::from_le_bytes(num(8)?);
        let mtime = i64::from_le_bytes(num(8)?);
        let size = u64::from_le_bytes(num(8)?);
        let mut opt = || -> Result<Option<String>> {
            let flag = *raw.get(at).ok_or(Errno::EINVAL)?;
            at += 1;
            if flag == 1 {
                get_str(raw, &mut at).map(Some)
            } else {
                Ok(None)
            }
        };
        let link_target = opt()?;
        let hard_link_of = opt()?;
        Ok(Self {
            path,
            objtype,
            meta,
            atime,
            mtime,
            size,
            link_target,
            hard_link_of,
        })
    }
}

/// What the save walk hands its writer.
pub trait SaveSink {
    /// One descriptor; for a stream file its data follows through
    /// [`SaveSink::data`].
    fn descriptor(&mut self, d: &Descriptor) -> Result<()>;
    /// A chunk of the current stream file's data.
    fn data(&mut self, chunk: &[u8]) -> Result<()>;
}

/// The objects a save skips: `*ALWSAV` off, and the file systems that
/// are not stored here (`/QSYS.LIB`, `/QOPT`, devices, unsupported).
fn skipped(s: &Stat, saveable: bool) -> bool {
    !saveable
        || !matches!(
            s.objtype.as_str(),
            "*STMF" | "*DIR" | "*SYMLNK" | "*FIFO" | "*SOCKET"
        )
}

impl<B: Backend> Ifs<B> {
    /// Save the subtree at `p` into `sink`: every link the caller may
    /// save (`*OBJEXIST` or `*SAVSYS`), parents first.
    pub fn save(&self, cx: &Ctx<'_>, p: &str, sink: &mut dyn SaveSink) -> Result<u64> {
        let mut first: std::collections::HashMap<(u32, u64), String> =
            std::collections::HashMap::new();
        let mut count = 0u64;
        let mut err: Option<Errno> = None;
        self.walk(
            cx,
            p,
            WalkOpts {
                follow: false,
                one_fs: false,
            },
            &mut |path, s| {
                let Ok(s) = s else { return Visit::Continue };
                let r = (|| -> Result<Visit> {
                    let (res, m) = self.resolve_meta(cx, path, false)?;
                    if skipped(s, m.flags.has(Flags::ALWSAV)) {
                        return Ok(if s.objtype == "*DIR" {
                            Visit::Continue
                        } else {
                            Visit::Skip
                        });
                    }
                    cx.require(&m, &res.node, path, crate::auth::need::Op::Save)?;
                    let hard = (s.objtype != "*DIR" && s.nlink > 1)
                        .then(|| first.get(&(s.vfs, s.ino)).cloned())
                        .flatten();
                    if s.objtype != "*DIR" && s.nlink > 1 && hard.is_none() {
                        first.insert((s.vfs, s.ino), path.to_string());
                    }
                    let link_target = (s.objtype == "*SYMLNK")
                        .then(|| self.readlink(cx, path))
                        .transpose()?;
                    let size = if s.objtype == "*STMF" && hard.is_none() {
                        s.size
                    } else {
                        0
                    };
                    sink.descriptor(&Descriptor {
                        path: path.to_string(),
                        objtype: s.objtype.clone(),
                        meta: codec::encode(&m),
                        atime: s.atime,
                        mtime: s.mtime,
                        size,
                        link_target,
                        hard_link_of: hard,
                    })?;
                    if size > 0 {
                        let fd = self.open(
                            cx,
                            path,
                            crate::ops::OpenArgs::new(crate::ops::flags::O_RDONLY),
                        )?;
                        let mut buf = vec![0u8; 64 * 1024];
                        let res = (|| loop {
                            let n = self.read(cx, fd, &mut buf)?;
                            if n == 0 {
                                return Ok(());
                            }
                            sink.data(&buf[..n])?;
                        })();
                        self.close(cx, fd)?;
                        res?;
                    }
                    count += 1;
                    Ok(Visit::Continue)
                })();
                match r {
                    Ok(v) => v,
                    Err(e) => {
                        err = Some(e);
                        Visit::Stop
                    }
                }
            },
        )?;
        match err {
            Some(e) => Err(e),
            None => Ok(count),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_round_trip() {
        let d = Descriptor {
            path: "/home/a/x.txt".into(),
            objtype: "*STMF".into(),
            meta: vec![1, 2, 3],
            atime: -5,
            mtime: 7,
            size: 42,
            link_target: None,
            hard_link_of: Some("/home/a/y.txt".into()),
        };
        assert_eq!(Descriptor::decode(&d.encode()).unwrap(), d);
        assert_eq!(Descriptor::decode(b"nope"), Err(Errno::EINVAL));
    }
}
