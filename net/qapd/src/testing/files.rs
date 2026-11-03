// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The test system's integrated file system: QS/400's own `ifs` crate
//! over a `MemBackend`, with its authority, locks and audit, so the
//! `Ifs` service answers in process exactly as on a system
//! ([`crate::files`] over the same `IfsClient` calls). Each connection
//! job is one owner of descriptors and locks; its threads run as the
//! signed-on profile.
//!
//! Fixtures, each a case a client meets:
//!
//! - `/home/BOB`, `/home/ANN`, `/home/WARN`, each owned by its profile
//!   (`*PUBLIC` `*RX`);
//! - [`MANY`]: 1,200 files `f0000.txt` to `f1199.txt`, for paging;
//! - [`TEXT_DIR`]: [`TEXT`] in CCSIDs 37, 273, 819 and 1208
//!   (`text37.txt` ...), [`JAPANESE`] in mixed 939 (`text939.txt`), and
//!   [`BINARY`] tagged 65535 (`binary.dat`), which never converts;
//! - [`LINK`] to `text1208.txt` and [`BROKEN`] to a file not there;
//! - [`PRIVATE`]: `ANN`'s directory with no `*PUBLIC` authority, which
//!   `BOB` may not list or read (`CPFA09C`);
//! - [`LOCKED`]: a file another job (owner [`HOLDER`], as `QSECOFR`)
//!   holds open with `O_SHARE_NONE`, so reading or writing it is
//!   `InUse` (`CPFA09E`); [`System::release_lock`] lets it go;
//! - [`CHECKED_OUT`]: a file of `BOB`'s checked out (`CHKOUT`) to
//!   `ANN`, so `BOB`'s `Delete`, `Rename` and `WriteFile` of it are
//!   `InUse` (`CPFA09E`) as `RMVLNK`, `MOV` and an open for writing
//!   answer; reading it is not refused;
//! - [`TREE`]: a folder of `BOB`'s with two levels of subfolders
//!   ([`TREE_DIRS`]) and files ([`TREE_FILES`]), one of them deep inside,
//!   [`TREE_HELD`], checked out to `ANN`: a depth-first delete of the
//!   tree removes the files before it and stops there with `InUse`;
//! - `/QSYS.LIB`: libraries `QGPL` (source file `QCLSRC` with member
//!   `HELLO`, a data area) and `HLINV` (a file `ASSETS`);
//! - [`BIG`]: a file of [`BIG_SIZE`] bytes (about 200 MiB, byte `i` is
//!   [`big_byte`]`(i)`), made the first time [`System::big_file`] is
//!   called and never otherwise.
//!
//! [`System::release_lock`]: super::System::release_lock
//! [`System::big_file`]: super::System::big_file

use std::sync::{Arc, Mutex};

use ifs::auth::MemEnv;
use ifs::backend::mem::MemBackend;
use ifs::client::{Direct, IfsClient};
use ifs::ops::audit::MemAudit;
use ifs::ops::flags::{O_CCSID, O_CREAT, O_RDWR, O_SHARE_NONE, O_TRUNC, O_WRONLY};
use ifs::ops::{Config, Ctx, Ifs, OpenArgs, Owner};
use ifs::vfs::qsys::{MemQsys, MemberData};
use sec::auth::bits::Auth;
use sec::auth::principal::Principal;
use sec::profile::spcaut::SpecialAuth;

use super::lock;

/// The directory of 1,200 files.
pub const MANY: &str = "/home/BOB/many";
/// How many files [`MANY`] has.
pub const MANY_COUNT: usize = 1200;
/// The text files.
pub const TEXT_DIR: &str = "/home/BOB/text";
/// The single-byte CCSIDs [`TEXT`] is stored in.
pub const TEXT_CCSIDS: [u16; 4] = [37, 273, 819, 1208];
/// Text every single-byte CCSID above holds.
pub const TEXT: &str = "Grüße aus Zürich: 1 + 1 = 2, ¡olé! [ok] {x} \\ ~\nZweite Zeile - äöü ÄÖÜ ß.\n";
/// Japanese, mixed single and double byte, for CCSID 939.
pub const JAPANESE: &str = "QS/400 は日本語を変換します。\nｶﾀｶﾅ ABC 123 東京都　千代田区\nabcあx 終わり\n";
/// The mixed CCSID.
pub const MIXED: u16 = 939;
/// The binary file's bytes.
pub const BINARY: &[u8] = &[0x00, 0x0E, 0xFF, 0x0F, 0x41, 0x80, 0xC1];
/// The binary file.
pub const BINARY_FILE: &str = "/home/BOB/text/binary.dat";
/// A symbolic link to `text1208.txt`.
pub const LINK: &str = "/home/BOB/link1208";
/// A symbolic link to nothing.
pub const BROKEN: &str = "/home/BOB/broken";
/// What [`BROKEN`] points to.
pub const BROKEN_TARGET: &str = "/home/BOB/gone.txt";
/// `ANN`'s directory `BOB` may not use.
pub const PRIVATE: &str = "/home/ANN/private";
/// A file in it.
pub const PRIVATE_FILE: &str = "/home/ANN/private/secret.txt";
/// A file another job holds.
pub const LOCKED: &str = "/home/BOB/locked.txt";
/// The owner (job) that holds [`LOCKED`].
pub const HOLDER: u64 = 1;
/// A file of `BOB`'s checked out to `ANN`.
pub const CHECKED_OUT: &str = "/home/BOB/checkedout.txt";
/// The profile [`CHECKED_OUT`] and [`TREE_HELD`] are checked out to.
pub const CHECKED_OUT_TO: &str = "ANN";
/// A folder tree of `BOB`'s.
pub const TREE: &str = "/home/BOB/tree";
/// Its subfolders, parents first.
pub const TREE_DIRS: [&str; 4] = [
    "/home/BOB/tree/docs",
    "/home/BOB/tree/docs/old",
    "/home/BOB/tree/src",
    "/home/BOB/tree/src/lib",
];
/// Its files but [`TREE_HELD`].
pub const TREE_FILES: [&str; 6] = [
    "/home/BOB/tree/readme.txt",
    "/home/BOB/tree/docs/guide.txt",
    "/home/BOB/tree/docs/old/notes.txt",
    "/home/BOB/tree/src/main.txt",
    "/home/BOB/tree/src/lib/a.txt",
    "/home/BOB/tree/src/lib/z.txt",
];
/// The file deep in [`TREE`] checked out to `ANN`.
pub const TREE_HELD: &str = "/home/BOB/tree/src/lib/m.txt";
/// The large file.
pub const BIG: &str = "/home/BOB/big.bin";
/// Its size: 200 MiB.
pub const BIG_SIZE: u64 = 200 << 20;

/// Byte `i` of [`BIG`].
#[must_use]
pub fn big_byte(i: u64) -> u8 {
    // Not a multiple of anything a chunk is, so misplaced bytes show.
    u8::try_from((i * 7 + i / 251) % 256).unwrap_or(0)
}

/// The path of [`TEXT`] in `ccsid` (or [`JAPANESE`] in [`MIXED`]).
#[must_use]
pub fn text_file(ccsid: u16) -> String {
    format!("{TEXT_DIR}/text{ccsid}.txt")
}

/// The profiles: name and UID.
const PROFILES: [(&str, u32); 6] = [("QSYS", 0), ("QSECOFR", 1), ("QDFTOWN", 3), ("BOB", 100), ("ANN", 101), ("WARN", 102)];

/// The profile `user` as the IFS checks it: `QSECOFR` with `*ALLOBJ`
/// and `*SECADM`, every other with none.
#[must_use]
pub fn principal(user: &str) -> Principal {
    let mut p = Principal {
        user: user.into(),
        qsecurity: 40,
        objaud: "*NONE".into(),
        ..Principal::default()
    };
    if user == "QSECOFR" || user == "QSYS" {
        p.spcaut = SpecialAuth::ALLOBJ.0 | SpecialAuth::SECADM.0 | SpecialAuth::IOSYSCFG.0 | SpecialAuth::AUDIT.0;
    }
    p
}

/// The file system and what it is checked against.
pub struct Files {
    /// The IFS.
    pub ifs: Arc<Ifs<MemBackend>>,
    /// Its profiles.
    pub env: Arc<MemEnv>,
    /// Its audit events.
    pub audit: Arc<MemAudit>,
    /// Whether [`BIG`] was made.
    big: Mutex<bool>,
}

impl Files {
    /// The file system with every fixture but [`BIG`].
    ///
    /// # Panics
    /// When a fixture cannot be made (a bug).
    #[must_use]
    pub fn new() -> Self {
        let q = MemQsys::new(Auth::USE);
        q.add_lib("QGPL");
        q.add_lib("HLINV");
        q.add_obj("QGPL", "QCLSRC", "*FILE", "PF-SRC", "QSYS");
        q.add_member(
            "QGPL",
            "QCLSRC",
            "HELLO",
            MemberData {
                records: ["PGM", "ENDPGM"]
                    .iter()
                    .map(|s| ccsid::to_ebcdic(&format!("{s:<20}")).unwrap_or_default())
                    .collect(),
                record_len: 20,
                source: true,
                ccsid: 37,
            },
        );
        q.add_obj("QGPL", "QAPDATA", "*DTAARA", "", "BOB");
        q.add_obj("HLINV", "ASSETS", "*FILE", "PF-DTA", "QSYS");
        let env = PROFILES.iter().fold(MemEnv::default(), |e, (n, uid)| e.user(n, *uid));
        let ifs = Ifs::new(MemBackend::new(), Box::new(q), Config::default());
        ifs.ensure_layout(&env).expect("layout");
        let f = Self {
            ifs: Arc::new(ifs),
            env: Arc::new(env),
            audit: Arc::new(MemAudit::default()),
            big: Mutex::new(false),
        };
        f.install();
        // Making the fixtures is not what tests audit.
        f.audit.take();
        f
    }

    /// Run `f` as `user`, job `owner`.
    pub fn as_user<T>(&self, user: &str, owner: u64, f: impl FnOnce(&dyn IfsClient) -> T) -> T {
        let p = principal(user);
        let cx = Ctx {
            owner: Owner(owner),
            principal: &p,
            env: &*self.env,
            audit: &*self.audit,
            job_ccsid: 37,
        };
        f(&Direct { ifs: &self.ifs, cx: &cx })
    }

    fn install(&self) {
        // The fixtures are made by an owner of their own.
        const MAKER: u64 = 2;
        self.as_user("QSECOFR", MAKER, |c| {
            for u in ["BOB", "ANN", "WARN"] {
                let home = format!("/home/{u}");
                c.mkdir(&home, 0o755).expect("home");
                c.chgown(&home, u, true).expect("home's owner");
            }
        });
        self.as_user("BOB", MAKER, |c| {
            c.mkdir(MANY, 0o755).expect("many");
            for i in 0..MANY_COUNT {
                put(c, &format!("{MANY}/f{i:04}.txt"), 1208, format!("file {i}\n").as_bytes());
            }
            c.mkdir(TEXT_DIR, 0o755).expect("text");
            for ccsid in TEXT_CCSIDS {
                let b = ccsid::conv::from_str(ccsid, TEXT).expect("text converts");
                put(c, &text_file(ccsid), ccsid, &b);
            }
            let b = ccsid::conv::from_str(MIXED, JAPANESE).expect("Japanese converts");
            put(c, &text_file(MIXED), MIXED, &b);
            put(c, BINARY_FILE, ccsid::conv::BINARY, BINARY);
            c.symlink(&text_file(1208), LINK).expect("link");
            c.symlink(BROKEN_TARGET, BROKEN).expect("broken link");
            put(c, LOCKED, 1208, b"held by another job\n");
            c.mkdir(TREE, 0o755).expect("tree");
            for d in TREE_DIRS {
                c.mkdir(d, 0o755).expect("tree folder");
            }
            for f in TREE_FILES {
                put(c, f, 1208, f.as_bytes());
            }
            // ANN may check out only what she may change.
            for f in [CHECKED_OUT, TREE_HELD] {
                put(c, f, 1208, b"checked out to ANN\n");
                c.chmod(f, 0o666).expect("ANN may change it");
            }
        });
        self.as_user(CHECKED_OUT_TO, MAKER, |c| {
            c.mkdir(PRIVATE, 0o700).expect("private");
            put(c, PRIVATE_FILE, 1208, b"ANN's only\n");
            for f in [CHECKED_OUT, TREE_HELD] {
                c.check_out(f).expect("checked out");
            }
        });
        self.hold_lock();
    }

    /// Job [`HOLDER`] opens [`LOCKED`] sharing nothing, and keeps it open.
    pub fn hold_lock(&self) {
        self.as_user("QSECOFR", HOLDER, |c| {
            c.open(LOCKED, OpenArgs::new(O_RDWR | O_SHARE_NONE)).expect("held");
        });
    }

    /// Job [`HOLDER`] lets every file go.
    pub fn release_lock(&self) {
        self.ifs.release_owner(Owner(HOLDER));
    }

    /// [`BIG`], made the first time it is asked for; its path.
    ///
    /// # Panics
    /// When it cannot be written (a bug).
    pub fn big_file(&self) -> &'static str {
        let mut made = lock(&self.big);
        if !*made {
            self.as_user("BOB", 2, |c| {
                let fd = c
                    .open(BIG, OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC | O_CCSID).mode(0o644).conv(65535))
                    .expect("big");
                let mut at = 0u64;
                let mut buf = Vec::with_capacity(ifs::client::CHUNK);
                while at < BIG_SIZE {
                    buf.clear();
                    let n = (BIG_SIZE - at).min(ifs::client::CHUNK as u64);
                    buf.extend((at..at + n).map(big_byte));
                    ifs::client::write_all(c, fd, &buf).expect("big data");
                    at += n;
                }
                c.close(fd).expect("big closed");
            });
            *made = true;
        }
        BIG
    }
}

impl Default for Files {
    fn default() -> Self {
        Self::new()
    }
}

/// A new file of `data`, tagged `ccsid`.
fn put(c: &dyn IfsClient, path: &str, ccsid: u16, data: &[u8]) {
    let fd = c
        .open(path, OpenArgs::new(O_WRONLY | O_CREAT | O_TRUNC | O_CCSID).mode(0o644).conv(u32::from(ccsid)))
        .unwrap_or_else(|e| panic!("{path}: {e:?}"));
    ifs::client::write_all(c, fd, data).expect("data");
    c.close(fd).expect("closed");
}
