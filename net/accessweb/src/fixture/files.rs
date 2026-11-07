// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prototype's integrated file system, in memory: `/home/TESTUSER`
//! with the entries of `web/files.html`, the shares of
//! `web/my-files.html`, and the transfer queue the page shows. The
//! owner has the owner's bits, everyone else the public bits, and
//! `*ALLOBJ` everything, as the system's own check does.

use std::collections::BTreeMap;
use std::sync::Mutex;

use qsweb::msg::{Kind as MsgKind, Message};

use super::{PROFILES, text};
use crate::env::files::{AuthChange, Entry, Files, Kind, Props, Share};
use crate::transfer::{Direction, Endings, Mode, Queue, State, Transfer};
use crate::util::secs;

/// The home directory.
pub const HOME: &str = "/home/TESTUSER";

/// `invload.clle`, as `web/file-edit.html` shows it.
pub const INVLOAD: &str = r#"/* INVLOAD - load inventory from CSV uploaded by Access for Web            */
/* Parameters: &MODE  *ALL | *NEW                                            */
             PGM        PARM(&MODE)
             DCL        VAR(&MODE)   TYPE(*CHAR) LEN(10)
             DCL        VAR(&FILE)   TYPE(*CHAR) LEN(64) +
                          VALUE('/home/TESTUSER/upload/assets_br1.csv')
             DCL        VAR(&MSG)    TYPE(*CHAR) LEN(80)

             CHKOBJ     OBJ(HLINV/ASSETS) OBJTYPE(*FILE)
             MONMSG     MSGID(CPF9801) EXEC(DO)
                SNDPGMMSG  MSGID(CPF9898) MSGF(QCPFMSG) +
                             MSGDTA('HLINV/ASSETS not found') MSGTYPE(*ESCAPE)
             ENDDO

             IF         COND(&MODE *EQ '*ALL') THEN(DO)
                CLRPFM     FILE(HLINV/ASSETSTG)
             ENDDO

             CPYFRMIMPF FROMSTMF(&FILE) TOFILE(HLINV/ASSETSTG) +
                          MBROPT(*ADD) RCDDLM(*LF) RMVBLANK(*BOTH) +
                          FLDDLM(',') STRDLM('"') RPLNULLVAL(*FLDDFT) +
                          ERRRCDFILE(HLINV/ASSETERR)
             MONMSG     MSGID(CPF2817) EXEC(DO)
                SNDPGMMSG  MSG('Import completed with errors, see HLINV/ASSETERR')
                GOTO       CMDLBL(END)
             ENDDO

             RUNSQL     SQL('MERGE INTO hlinv.assets a USING hlinv.assetstg s +
                          ON a.asset_tag = s.asset_tag +
                          WHEN MATCHED THEN UPDATE SET status = s.status, +
                            location = s.location, changed_ts = CURRENT TIMESTAMP +
                          WHEN NOT MATCHED THEN INSERT VALUES (s.asset_tag, +
                            s.location, s.manufacturer, s.model, s.serial, +
                            s.asset_type, s.status, s.acquired, s.cost, s.rack_u, +
                            s.notes, CURRENT TIMESTAMP)') COMMIT(*NONE)

             CHGVAR     VAR(&MSG) VALUE('INVLOAD completed, mode ' *CAT &MODE)
             SNDMSG     MSG(&MSG) TOUSR(TESTUSER)
 END:        ENDPGM
"#;

/// What a node holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Data {
    /// A directory.
    Dir,
    /// A stream file's bytes.
    File(Vec<u8>),
    /// A symbolic link's target.
    Link(String),
}

/// One object of the sample file system.
#[derive(Clone, Debug)]
pub struct Node {
    /// What it holds.
    pub data: Data,
    /// The owner.
    pub owner: String,
    /// Permission bits.
    pub mode: u32,
    /// The data's CCSID.
    pub ccsid: u16,
    /// Created, modified, last used.
    pub times: (u64, u64, u64),
    /// Change count.
    pub changes: u64,
    /// Authorization list.
    pub autl: String,
}

/// A local time on the sample system (US Eastern daylight time).
fn local(mo: u32, d: u32, h: u32, mi: u32, s: u32) -> u64 {
    secs(2026, mo, d, h, mi, s) + 4 * 3600
}

/// `n` bytes of text made of `line` over and over, ending in a newline.
fn text_of(line: &str, n: usize) -> Vec<u8> {
    let mut v: Vec<u8> = line.bytes().cycle().take(n.saturating_sub(1)).collect();
    v.push(b'\n');
    v
}

/// `n` bytes that are not text: `head`, then a counting pattern with
/// NULs in it.
fn binary_of(head: &[u8], n: usize) -> Vec<u8> {
    let mut v = head.to_vec();
    let mut i: u32 = 0;
    while v.len() < n {
        v.push(if i.is_multiple_of(7) {
            0
        } else {
            u8::try_from(i % 251).unwrap_or(0)
        });
        i += 1;
    }
    v.truncate(n);
    v
}

fn node(data: Data, mode: u32, ccsid: u16, at: u64) -> Node {
    Node {
        data,
        owner: "TESTUSER".into(),
        mode,
        ccsid,
        times: (at, at, at),
        changes: 1,
        autl: "*NONE".into(),
    }
}

fn dir(mode: u32, at: u64) -> Node {
    node(Data::Dir, mode, 0, at)
}

/// The sample tree: path to node.
#[allow(clippy::too_many_lines)]
pub fn tree() -> BTreeMap<String, Node> {
    let mut t = BTreeMap::new();
    let early = local(9, 1, 8, 0, 0);
    let mut put = |p: &str, n: Node| {
        t.insert(p.to_string(), n);
    };
    let sys = |mut n: Node| {
        n.owner = "QSYS".into();
        n
    };
    put("/", sys(dir(0o755, early)));
    put("/home", sys(dir(0o755, early)));
    put("/QS400", sys(dir(0o755, early)));
    put("/QS400/UserData", sys(dir(0o755, early)));
    put("/QS400/UserData/Shares", sys(dir(0o755, early)));
    put("/QS400/UserData/Shares/docs", sys(dir(0o777, early)));
    put(
        "/QS400/UserData/Shares/docs/reports",
        dir(0o777, local(10, 1, 9, 0, 0)),
    );
    put("/QSYS.LIB", sys(dir(0o755, early)));
    put("/QSYS.LIB/HLINV.LIB", dir(0o750, early));
    put("/www", sys(dir(0o755, early)));
    put("/www/webstore", sys(dir(0o755, early)));
    put("/www/webstore/htdocs", sys(dir(0o755, early)));
    put(HOME, dir(0o700, early));
    put("/home/TESTUSER/certs", dir(0o700, local(10, 20, 11, 2, 0)));
    put(
        "/home/TESTUSER/scripts",
        dir(0o750, local(10, 28, 17, 40, 0)),
    );
    put(
        "/home/TESTUSER/upload",
        dir(0o770, local(10, 29, 13, 30, 0)),
    );
    put(
        "/home/TESTUSER/.profile",
        node(
            Data::File(text_of(
                "export PATH=/QOpenSys/usr/bin:/usr/bin:$PATH\n",
                412,
            )),
            0o600,
            1208,
            early,
        ),
    );
    let mut csv = node(
        Data::File(text_of(
            "QS000101,QS-HQ-R1,QUANTA,T4048-IX8,QTFCEV2340021,SWITCH,ACTIVE\n",
            2118,
        )),
        0o640,
        1208,
        local(10, 29, 13, 30, 2),
    );
    csv.times.2 = local(10, 29, 13, 58, 10);
    csv.changes = 3;
    put("/home/TESTUSER/assets_br1.csv", csv);
    put(
        "/home/TESTUSER/inventory_2026-10.pdf",
        node(
            Data::File(binary_of(b"%PDF-1.4\n", 240_112)),
            0o644,
            65535,
            local(10, 28, 18, 2, 0),
        ),
    );
    put(
        "/home/TESTUSER/invload.clle",
        node(
            Data::File(INVLOAD.as_bytes().to_vec()),
            0o640,
            1208,
            local(10, 29, 13, 5, 0),
        ),
    );
    put(
        "/home/TESTUSER/notes.txt",
        node(
            Data::File(text_of(
                "Rack QS-HQ-R3 gets the lab build box next week.\n",
                9310,
            )),
            0o600,
            1208,
            local(10, 27, 21, 15, 0),
        ),
    );
    put(
        "/home/TESTUSER/reports",
        node(
            Data::Link("/QS400/UserData/Shares/docs/reports".into()),
            0o777,
            0,
            local(10, 1, 9, 0, 0),
        ),
    );
    put(
        "/home/TESTUSER/old-link",
        node(Data::Link("/home/TESTUSER/gone".into()), 0o777, 0, early),
    );
    put(
        "/home/TESTUSER/webstore.p12",
        node(
            Data::File(binary_of(&[0x30, 0x82, 0x0d, 0x46], 3402)),
            0o600,
            65535,
            local(10, 20, 11, 2, 0),
        ),
    );
    let up = local(10, 29, 13, 30, 0);
    put(
        "/home/TESTUSER/upload/assets_br2.csv",
        node(
            Data::File(text_of("QS000401,QS-BR1-R1,DELL,S4048-ON\n", 41_200)),
            0o640,
            1208,
            up,
        ),
    );
    put(
        "/home/TESTUSER/upload/old.csv",
        node(
            Data::File(text_of("QS000110,QS-HQ-R1\n", 640)),
            0o640,
            1208,
            up,
        ),
    );
    put(
        "/home/TESTUSER/upload/old2.csv",
        node(
            Data::File(text_of("QS000111,QS-HQ-R1\n", 320)),
            0o640,
            1208,
            up,
        ),
    );
    t
}

/// The queue `web/files.html` shows.
pub fn sample_queue() -> Queue {
    let q = Queue::default();
    let mut t = Transfer::new(
        "/home/TESTUSER/upload/assets_br2.csv",
        Direction::Upload,
        Mode::Text(1208, Endings::Lf),
        41_200,
    );
    t.done = 41_200;
    t.state = State::Complete;
    q.add(super::USER, t);
    let mut t = Transfer::new(
        "/home/TESTUSER/inventory_2026-10.pdf",
        Direction::Download,
        Mode::Binary,
        240_112,
    );
    t.done = 128_000;
    t.state = State::Running(Some("inventory_2026-10.pdf.qsapart".into()));
    q.add(super::USER, t);
    let mut t = Transfer::new(
        "/home/TESTUSER/scripts/backup.sh",
        Direction::Upload,
        Mode::Binary,
        2048,
    );
    t.state = State::Failed("QAD8021".into(), "not authorized".into());
    t.data = Some(std::sync::Arc::new(text_of(
        "#!/bin/sh\nSAV DEV('/QSYS.LIB/QGPL.LIB/BKUP.FILE')\n",
        2048,
    )));
    q.add(super::USER, t);
    q
}

/// The sample file system.
pub struct FixtureFiles {
    /// Path to node.
    pub tree: Mutex<BTreeMap<String, Node>>,
    /// Transfers.
    pub queue: Queue,
}

impl Default for FixtureFiles {
    fn default() -> Self {
        Self {
            tree: Mutex::new(tree()),
            queue: sample_queue(),
        }
    }
}

/// A system message from the sample system's texts.
fn msg(id: &str, data: &[&str]) -> Message {
    let t = text::second_level(id, data)
        .map(|s| s.text)
        .unwrap_or_default();
    Message::new(MsgKind::Error, id, &t).with_data(data)
}

fn parent(p: &str) -> &str {
    match p.rfind('/') {
        Some(i) if i > 0 => &p[..i],
        _ => "/",
    }
}

fn name(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

/// The path normalised: one slash between parts, no trailing slash.
fn clean(p: &str) -> String {
    let parts: Vec<&str> = p
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();
    format!("/{}", parts.join("/"))
}

impl FixtureFiles {
    fn all_obj(user: &str) -> bool {
        PROFILES
            .iter()
            .any(|p| p.name == user && p.special.contains(&"*ALLOBJ"))
    }

    /// Whether `user` has `bit` (4 read, 2 write, 1 execute) to `n`.
    fn may(user: &str, n: &Node, bit: u32) -> bool {
        let bits = if n.owner == user { n.mode >> 6 } else { n.mode };
        Self::all_obj(user) || bits & bit != 0
    }

    /// `path` followed through links.
    fn resolve(t: &BTreeMap<String, Node>, path: &str) -> String {
        let mut p = clean(path);
        for _ in 0..8 {
            match t.get(&p).map(|n| &n.data) {
                Some(Data::Link(to)) => p = clean(to),
                _ => break,
            }
        }
        p
    }

    fn get(&self, user: &str, path: &str, bit: u32) -> Result<(String, Node), Message> {
        let t = self.tree.lock().expect("tree");
        let p = Self::resolve(&t, path);
        let n = t.get(&p).ok_or_else(|| msg("CPFA0A9", &[path]))?;
        if !Self::may(user, n, bit) {
            return Err(msg("CPFA09C", &[path]));
        }
        Ok((p, n.clone()))
    }

    /// Check `user` may make or remove entries of `path`'s directory.
    fn writable_parent(t: &BTreeMap<String, Node>, user: &str, path: &str) -> Result<(), Message> {
        let d = Self::resolve(t, parent(path));
        let n = t.get(&d).ok_or_else(|| msg("CPFA0A9", &[parent(path)]))?;
        if n.data != Data::Dir {
            return Err(msg("CPFA0A9", &[parent(path)]));
        }
        if Self::may(user, n, 2) {
            Ok(())
        } else {
            Err(msg("CPFA09C", &[path]))
        }
    }

    fn entry_of(t: &BTreeMap<String, Node>, path: &str, n: &Node) -> Entry {
        let (kind, size) = match &n.data {
            Data::Dir => (Kind::Dir, 0),
            Data::File(b) => (Kind::File, b.len() as u64),
            Data::Link(to) => {
                let target = Self::resolve(t, to);
                let at = t.get(&target);
                (
                    Kind::Link {
                        target: to.clone(),
                        dir: at.is_some_and(|n| n.data == Data::Dir),
                        broken: at.is_none(),
                    },
                    0,
                )
            }
        };
        Entry {
            name: name(path).to_string(),
            kind,
            size,
            modified: n.times.1,
            owner: n.owner.clone(),
            mode: n.mode,
            ccsid: n.ccsid,
        }
    }

    fn under(t: &BTreeMap<String, Node>, path: &str) -> Vec<String> {
        let pre = format!("{}/", path.trim_end_matches('/'));
        t.keys().filter(|k| k.starts_with(&pre)).cloned().collect()
    }
}

impl Files for FixtureFiles {
    fn home(&self, user: &str) -> String {
        format!("/home/{user}")
    }

    fn list(&self, user: &str, dir: &str) -> Result<Vec<Entry>, Message> {
        let (p, n) = self.get(user, dir, 4)?;
        if n.data != Data::Dir {
            return Err(msg("CPFA0A9", &[dir]));
        }
        let t = self.tree.lock().expect("tree");
        let pre = if p == "/" {
            "/".to_string()
        } else {
            format!("{p}/")
        };
        Ok(t.iter()
            .filter(|(k, _)| {
                k.len() > pre.len() && k.starts_with(&pre) && !k[pre.len()..].contains('/')
            })
            .map(|(k, n)| Self::entry_of(&t, k, n))
            .collect())
    }

    fn entry(&self, _user: &str, path: &str) -> Result<Entry, Message> {
        let t = self.tree.lock().expect("tree");
        let p = clean(path);
        t.get(&p)
            .map(|n| Self::entry_of(&t, &p, n))
            .ok_or_else(|| msg("CPFA0A9", &[path]))
    }

    fn props(&self, user: &str, path: &str) -> Result<Props, Message> {
        let e = self.entry(user, path)?;
        let t = self.tree.lock().expect("tree");
        let n = t.get(&clean(path)).ok_or_else(|| msg("CPFA0A9", &[path]))?;
        let typ = match n.data {
            Data::Dir => "Directory (*DIR)",
            Data::File(_) => "Stream file (*STMF)",
            Data::Link(_) => "Symbolic link (*SYMLNK)",
        };
        Ok(Props {
            entry: e,
            typ: typ.into(),
            created: n.times.0,
            accessed: n.times.2,
            changes: n.changes,
            group: "*NONE".into(),
            autl: n.autl.clone(),
        })
    }

    fn read(&self, user: &str, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, Message> {
        let (_, n) = self.get(user, path, 4)?;
        let Data::File(b) = n.data else {
            return Err(msg("CPFA0DA", &[path]));
        };
        let start = usize::try_from(offset).unwrap_or(usize::MAX).min(b.len());
        let end = start
            .saturating_add(usize::try_from(len).unwrap_or(usize::MAX))
            .min(b.len());
        Ok(b[start..end].to_vec())
    }

    fn write(&self, user: &str, path: &str, data: &[u8], ccsid: u16) -> Result<(), Message> {
        let mut t = self.tree.lock().expect("tree");
        let p = Self::resolve(&t, path);
        let now = super::now();
        if let Some(n) = t.get_mut(&p) {
            if n.data == Data::Dir {
                return Err(msg("CPFA0DA", &[path]));
            }
            if !Self::may(user, n, 2) {
                return Err(msg("CPFA09C", &[path]));
            }
            n.data = Data::File(data.to_vec());
            n.times.1 = now;
            n.changes += 1;
            return Ok(());
        }
        Self::writable_parent(&t, user, &p)?;
        let mut n = node(Data::File(data.to_vec()), 0o640, ccsid, now);
        n.owner = user.to_string();
        t.insert(p, n);
        Ok(())
    }

    fn mkdir(&self, user: &str, path: &str) -> Result<(), Message> {
        let mut t = self.tree.lock().expect("tree");
        let p = clean(path);
        if t.contains_key(&p) {
            return Err(msg("CPFA0A0", &[path]));
        }
        Self::writable_parent(&t, user, &p)?;
        let mut n = dir(0o750, super::now());
        n.owner = user.to_string();
        t.insert(p, n);
        Ok(())
    }

    fn rename(&self, user: &str, from: &str, to: &str) -> Result<(), Message> {
        let mut t = self.tree.lock().expect("tree");
        let (f, to_p) = (clean(from), clean(to));
        if !t.contains_key(&f) {
            return Err(msg("CPFA0A9", &[from]));
        }
        if t.contains_key(&to_p) {
            return Err(msg("CPFA0A0", &[to]));
        }
        Self::writable_parent(&t, user, &f)?;
        Self::writable_parent(&t, user, &to_p)?;
        let mut moved = vec![(f.clone(), to_p.clone())];
        for k in Self::under(&t, &f) {
            moved.push((k.clone(), format!("{to_p}{}", &k[f.len()..])));
        }
        for (a, b) in moved {
            if let Some(n) = t.remove(&a) {
                t.insert(b, n);
            }
        }
        Ok(())
    }

    fn copy(&self, user: &str, from: &str, to: &str) -> Result<(), Message> {
        let mut t = self.tree.lock().expect("tree");
        let (f, to_p) = (clean(from), clean(to));
        let n = t.get(&f).cloned().ok_or_else(|| msg("CPFA0A9", &[from]))?;
        if !Self::may(user, &n, 4) {
            return Err(msg("CPFA09C", &[from]));
        }
        if t.contains_key(&to_p) {
            return Err(msg("CPFA0A0", &[to]));
        }
        Self::writable_parent(&t, user, &to_p)?;
        let mut add = vec![(to_p.clone(), n)];
        for k in Self::under(&t, &f) {
            if let Some(n) = t.get(&k) {
                add.push((format!("{to_p}{}", &k[f.len()..]), n.clone()));
            }
        }
        for (k, mut n) in add {
            n.owner = user.to_string();
            t.insert(k, n);
        }
        Ok(())
    }

    fn delete(&self, user: &str, path: &str) -> Result<u64, (String, Message, u64)> {
        let mut t = self.tree.lock().expect("tree");
        let p = clean(path);
        if !t.contains_key(&p) {
            return Err((path.to_string(), msg("CPFA0A9", &[path]), 0));
        }
        // From the inside out, as the system deletes a tree.
        let mut all = Self::under(&t, &p);
        all.sort_by_key(|k| std::cmp::Reverse(k.len()));
        all.push(p);
        let mut done = 0;
        for k in all {
            if let Err(m) = Self::writable_parent(&t, user, &k) {
                return Err((k, m, done));
            }
            t.remove(&k);
            done += 1;
        }
        Ok(done)
    }

    fn count(&self, _user: &str, path: &str) -> Result<u64, Message> {
        let t = self.tree.lock().expect("tree");
        let p = clean(path);
        if !t.contains_key(&p) {
            return Err(msg("CPFA0A9", &[path]));
        }
        Ok(Self::under(&t, &p).len() as u64 + 1)
    }

    fn set_auth(&self, user: &str, path: &str, change: &AuthChange) -> Result<(), Message> {
        let mut t = self.tree.lock().expect("tree");
        let p = clean(path);
        let n = t.get_mut(&p).ok_or_else(|| msg("CPFA0A9", &[path]))?;
        if n.owner != user && !Self::all_obj(user) {
            return Err(msg("CPFA09C", &[path]));
        }
        n.mode = change.mode & 0o777;
        n.autl.clone_from(&change.autl);
        n.changes += 1;
        Ok(())
    }

    fn shares(&self, user: &str) -> Vec<Share> {
        let home = self.home(user);
        let mut v = vec![Share {
            name: "Home".into(),
            path: home,
            typ: "Directory".into(),
            used: Some(12_400_000),
        }];
        if user == super::USER {
            v.push(Share {
                name: "Shared documents".into(),
                path: "/QS400/UserData/Shares/docs".into(),
                typ: "Share (SMB: DOCS)".into(),
                used: Some(4_100_000_000),
            });
            v.push(Share {
                name: "Web store content".into(),
                path: "/www/webstore/htdocs".into(),
                typ: "Directory".into(),
                used: Some(88_000_000),
            });
            v.push(Share {
                name: "Inventory library".into(),
                path: "/QSYS.LIB/HLINV.LIB".into(),
                typ: "Library".into(),
                used: Some(88_100_000),
            });
        }
        v
    }

    fn queue(&self) -> &Queue {
        &self.queue
    }
}
