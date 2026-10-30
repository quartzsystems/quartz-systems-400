// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The host servers' exit point formats (`EZSOEP`, `EZSCEP`, `EZDAEP`,
//! `EZHQEP`, `EPWFSEP`, `ENPSEP`, `EZRCEP`). Each program gets two
//! parameters: the return code, `CHAR(1)` (`1` allows), and the
//! request's structure.

use crate::spec::{Layout, Rec, field};
use crate::text;

/// Server identifiers as the headers define them.
pub mod server {
    /// Sign-on server.
    pub const SIGNON: &str = "*SIGNON";
    /// Central server.
    pub const CENTRAL: &str = "*CNTRLSRV";
    /// Database server, initialization and SQL.
    pub const SQL: &str = "*SQL";
    /// Database server, native database.
    pub const NDB: &str = "*NDB";
    /// Database server, object information.
    pub const ROI: &str = "*ROI";
    /// Data queue server.
    pub const DATAQ: &str = "*DATAQSRV";
    /// File server.
    pub const FILE: &str = "*FILESRV";
    /// Network print server.
    pub const NETPRT: &str = "QNPSERVR";
    /// Remote command and program call server.
    pub const RMTCMD: &str = "*RMTSRV";
}

/// Function identifiers the headers define.
pub mod func {
    /// Sign-on: start server.
    pub const START_SERVER: i32 = 0x7002;
    /// Sign-on: retrieve sign-on information.
    pub const RETRIEVE_SIGNON: i32 = 0x7004;
    /// Sign-on: change password.
    pub const CHANGE_PASSWORD: i32 = 0x7005;
    /// Sign-on: generate an authentication token.
    pub const GENERATE_TOKEN: i32 = 0x7007;
    /// Sign-on: generate a token for another user.
    pub const GENERATE_TOKEN_FOR_USER: i32 = 0x7008;
    /// Central: request a license.
    pub const REQUEST_LICENSE: i32 = 0x1001;
    /// Central: release a license.
    pub const RELEASE_LICENSE: i32 = 0x1002;
    /// Central: retrieve license information.
    pub const RTV_LICENSE_INFO: i32 = 0x1003;
    /// Central: set client active.
    pub const SET_CLIENT_ACTIVE: i32 = 0x1101;
    /// Central: set client inactive.
    pub const SET_CLIENT_INACTIVE: i32 = 0x1102;
    /// Central: retrieve a conversion map.
    pub const RTV_CONVERSION_MAP: i32 = 0x1201;
    /// Database: initialization.
    pub const DB_INIT: i32 = 0x0000;
    /// Remote command.
    pub const RMT_COMMAND: i32 = 0x1002;
    /// Remote program call.
    pub const RMT_PROGRAM_CALL: i32 = 0x1003;
    /// Network print: entry.
    pub const NPS_ENTRY: i32 = 0x0802;
    /// Network print: a spooled file.
    pub const NPS_SPLF: i32 = 0x010D;
    /// File server: change.
    pub const FILE_CHANGE: i32 = 0;
    /// File server: create.
    pub const FILE_CREATE: i32 = 1;
    /// File server: delete.
    pub const FILE_DELETE: i32 = 2;
    /// File server: list attributes.
    pub const FILE_LIST_ATTR: i32 = 3;
    /// File server: move.
    pub const FILE_MOVE: i32 = 4;
    /// File server: open.
    pub const FILE_OPEN: i32 = 5;
    /// File server: rename.
    pub const FILE_RENAME: i32 = 6;
    /// File server: allocate a conversation.
    pub const FILE_ALLOC_CONV: i32 = 7;
    /// File server: copy.
    pub const FILE_COPY: i32 = 8;
    /// Data queue: query attributes.
    pub const DQ_QUERY: i32 = 0x0001;
    /// Data queue: receive.
    pub const DQ_RECEIVE: i32 = 0x0002;
    /// Data queue: create.
    pub const DQ_CREATE: i32 = 0x0003;
    /// Data queue: delete.
    pub const DQ_DELETE: i32 = 0x0004;
    /// Data queue: send.
    pub const DQ_SEND: i32 = 0x0005;
    /// Data queue: clear.
    pub const DQ_CLEAR: i32 = 0x0006;
    /// Data queue: cancel a pending receive.
    pub const DQ_CANCEL: i32 = 0x0007;
}

/// `ZSOY0100`: sign-on server.
pub static ZSOY0100: Layout = Layout {
    name: "ZSOY0100",
    size: 32,
    fields: &[
        field("USRPRF", 0, 10),
        field("APPNAME", 10, 10),
        field("FMTNAME", 20, 8),
        field("FUNCID", 28, 4),
    ],
};

/// `ZSCL0100`: central server, license management.
pub static ZSCL0100: Layout = Layout {
    name: "ZSCL0100",
    size: 314,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FMTNAME", 20, 8),
        field("FUNCID", 28, 4),
        field("CLIENT", 32, 255),
        field("HANDLE", 287, 8),
        field("PRODID", 295, 7),
        field("FEATURE", 302, 4),
        field("RELEASE", 306, 6),
        field("INFOTYPE", 312, 2),
    ],
};

/// `ZSCS0100`: central server, client management.
pub static ZSCS0100: Layout = Layout {
    name: "ZSCS0100",
    size: 798,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FMTNAME", 20, 8),
        field("FUNCID", 28, 4),
        field("CLIENT", 32, 255),
        field("CMNTY", 287, 255),
        field("NODETYPE", 542, 1),
        field("NODENAME", 543, 255),
    ],
};

/// `ZSCN0100`: central server, conversion maps.
pub static ZSCN0100: Layout = Layout {
    name: "ZSCN0100",
    size: 42,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FMTNAME", 20, 8),
        field("FUNCID", 28, 4),
        field("FROMCCSID", 32, 4),
        field("TOCCSID", 36, 4),
        field("MAPTYPE", 40, 2),
    ],
};

/// `ZDAI0100`: database server initialization.
pub static ZDAI0100: Layout = Layout {
    name: "ZDAI0100",
    size: 285,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FMTNAME", 20, 8),
        field("FUNCID", 28, 4),
        field("IFCTYPE", 32, 63),
        field("IFCNAME", 95, 127),
        field("IFCLEVEL", 222, 63),
    ],
};

/// `ZDAD0100`: database server, native database requests.
pub static ZDAD0100: Layout = Layout {
    name: "ZDAD0100",
    size: 358,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FMTNAME", 20, 8),
        field("FUNCID", 28, 4),
        field("FILE", 32, 128),
        field("LIB", 160, 10),
        field("MBR", 170, 10),
        field("AUT", 180, 10),
        field("BASEFILE", 190, 128),
        field("BASELIB", 318, 10),
        field("OVRFILE", 328, 10),
        field("OVRLIB", 338, 10),
        field("OVRMBR", 348, 10),
    ],
};

/// `ZDAD0200`: database server, add library list; the libraries
/// follow, ten bytes each.
pub static ZDAD0200: Layout = Layout {
    name: "ZDAD0200",
    size: 36,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FMTNAME", 20, 8),
        field("FUNCID", 28, 4),
        field("NBRLIBS", 32, 4),
    ],
};

/// `ZDAR0100`: database server, object information.
pub static ZDAR0100: Layout = Layout {
    name: "ZDAR0100",
    size: 404,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FMTNAME", 20, 8),
        field("FUNCID", 28, 4),
        field("LIB", 32, 20),
        field("RDB", 52, 36),
        field("PKG", 88, 20),
        field("FILE", 108, 256),
        field("MBR", 364, 20),
        field("FMT", 384, 20),
    ],
};

/// `ZDAR0200`: database server, primary and foreign keys.
pub static ZDAR0200: Layout = Layout {
    name: "ZDAR0200",
    size: 308,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FMTNAME", 20, 8),
        field("FUNCID", 28, 4),
        field("PKLIB", 32, 10),
        field("PKNAME", 42, 128),
        field("FKLIB", 170, 10),
        field("FKNAME", 180, 128),
    ],
};

/// `ZDAQ0100`: database server SQL, the first 512 bytes of the text.
pub static ZDAQ0100: Layout = Layout {
    name: "ZDAQ0100",
    size: 865,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FMTNAME", 20, 8),
        field("FUNCID", 28, 4),
        field("STMTNAME", 32, 18),
        field("CURSOR", 50, 18),
        field("PREPOPT", 68, 2),
        field("OPENATR", 70, 2),
        field("PKG", 72, 10),
        field("PKGLIB", 82, 10),
        field("DRDA", 92, 2),
        field("COMMIT", 94, 1),
        field("SQLTEXT", 95, 512),
        field("EXTCURSOR", 607, 258),
    ],
};

/// `ZDAQ0200`: database server SQL; the whole text follows, then the
/// extended cursor name at its offset.
pub static ZDAQ0200: Layout = Layout {
    name: "ZDAQ0200",
    size: 238,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FMTNAME", 20, 8),
        field("FUNCID", 28, 4),
        field("STMTNAME", 32, 18),
        field("CURSOR", 50, 18),
        field("PREPOPT", 68, 2),
        field("OPENATR", 70, 2),
        field("PKG", 72, 10),
        field("PKGLIB", 82, 10),
        field("DRDA", 92, 2),
        field("COMMIT", 94, 1),
        field("DFTLIB", 95, 10),
        field("RSV1", 105, 3),
        field("OFFEXTCUR", 108, 4),
        field("LENEXTCUR", 112, 4),
        field("RSV2", 116, 118),
        field("SQLLEN", 234, 4),
    ],
};

/// `ZHQ00100`: data queue server.
pub static ZHQ00100: Layout = Layout {
    name: "ZHQ00100",
    size: 314,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FMTNAME", 20, 8),
        field("FUNCID", 28, 4),
        field("OBJ", 32, 10),
        field("LIB", 42, 10),
        field("KEYOP", 52, 2),
        field("KEYLEN", 54, 4),
        field("KEY", 58, 256),
    ],
};

/// `PWFS0100`: file server; the file name follows.
pub static PWFS0100: Layout = Layout {
    name: "PWFS0100",
    size: 40,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FUNCID", 20, 4),
        field("FMTNAME", 24, 8),
        field("READ", 32, 1),
        field("WRITE", 33, 1),
        field("READWRITE", 34, 1),
        field("DELETE", 35, 1),
        field("NAMELEN", 36, 4),
    ],
};

/// `PWFS0200`: file server with a source and a target name, which
/// follow at their offsets.
pub static PWFS0200: Layout = Layout {
    name: "PWFS0200",
    size: 132,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FUNCID", 20, 4),
        field("FMTNAME", 24, 8),
        field("READ", 32, 1),
        field("WRITE", 33, 1),
        field("READWRITE", 34, 1),
        field("DELETE", 35, 1),
        field("OBJTYPE", 36, 10),
        field("RSV1", 46, 6),
        field("SRCOFF", 52, 4),
        field("SRCLEN", 56, 4),
        field("TGTOFF", 60, 4),
        field("TGTLEN", 64, 4),
        field("RSV2", 68, 64),
    ],
};

/// `ENTR0100`: network print server entry.
pub static ENTR0100: Layout = Layout {
    name: "ENTR0100",
    size: 32,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FMTNAME", 20, 8),
        field("FUNCID", 28, 4),
    ],
};

/// `SPLF0100`: network print server, a spooled file; the exit program
/// data follows its length.
pub static SPLF0100: Layout = Layout {
    name: "SPLF0100",
    size: 97,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FMTNAME", 20, 8),
        field("FUNCID", 28, 4),
        field("JOBNAME", 32, 10),
        field("JOBUSER", 42, 10),
        field("JOBNBR", 52, 6),
        field("SPLFNAME", 58, 10),
        field("SPLFNBR", 68, 4),
        field("JOBSYS", 72, 8),
        field("CRTDATE", 80, 7),
        field("CRTTIME", 87, 6),
        field("DATALEN", 93, 4),
    ],
};

/// `CZRC0100`: remote command and program call. `CCSID` overlays
/// `OBJ` for a command; the command or the program's parameters
/// follow, the structure padded to IBM's 6056 bytes.
pub static CZRC0100: Layout = Layout {
    name: "CZRC0100",
    size: 56,
    fields: &[
        field("USRPRF", 0, 10),
        field("SRVID", 10, 10),
        field("FMTNAME", 20, 8),
        field("FUNCID", 28, 4),
        field("OBJ", 32, 10),
        field("LIB", 42, 10),
        field("LENOTHER", 52, 4),
    ],
};

/// The size IBM declares for `CZRC0100`, `Other` included.
pub const CZRC0100_DECLARED: usize = 6056;

/// The host server layouts.
pub static LAYOUTS: &[&Layout] = &[
    &ZSOY0100, &ZSCL0100, &ZSCS0100, &ZSCN0100, &ZDAI0100, &ZDAD0100, &ZDAD0200, &ZDAR0100,
    &ZDAR0200, &ZDAQ0100, &ZDAQ0200, &ZHQ00100, &PWFS0100, &PWFS0200, &ENTR0100, &SPLF0100,
    &CZRC0100,
];

/// Who asks: the user profile and the function, for every host format.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Who {
    /// The user profile the server runs the request as.
    pub user: String,
    /// The requested function (`func`).
    pub function: i32,
}

fn header(l: &Layout, who: &Who, server: &str) -> Rec {
    let mut r = l.blank();
    r.text("USRPRF", &who.user)
        .text("SRVID", server)
        .text("FMTNAME", l.name)
        .bin4("FUNCID", who.function);
    r
}

fn n32(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

/// `ZSOY0100`.
#[must_use]
pub fn zsoy0100(who: &Who) -> Vec<u8> {
    let mut r = ZSOY0100.blank();
    r.text("USRPRF", &who.user)
        .text("APPNAME", server::SIGNON)
        .text("FMTNAME", "ZSOY0100")
        .bin4("FUNCID", who.function);
    r.into_bytes()
}

/// A license request (`ZSCL0100`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct License {
    /// Client name.
    pub client: String,
    /// License user handle.
    pub handle: [u8; 8],
    /// Product, feature, release.
    pub product: (String, String, String),
    /// Information type: 0 basic, 1 detailed.
    pub info_type: i16,
}

/// `ZSCL0100`.
#[must_use]
pub fn zscl0100(who: &Who, l: &License) -> Vec<u8> {
    let mut r = header(&ZSCL0100, who, server::CENTRAL);
    r.text("CLIENT", &l.client)
        .bytes("HANDLE", &l.handle)
        .text("PRODID", &l.product.0)
        .text("FEATURE", &l.product.1)
        .text("RELEASE", &l.product.2)
        .bin2("INFOTYPE", l.info_type);
    r.into_bytes()
}

/// A client management request (`ZSCS0100`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Client {
    /// Client name.
    pub client: String,
    /// Community name.
    pub community: String,
    /// Node type (`3` host address).
    pub node_type: char,
    /// Node name.
    pub node: String,
}

/// `ZSCS0100`.
#[must_use]
pub fn zscs0100(who: &Who, c: &Client) -> Vec<u8> {
    let mut r = header(&ZSCS0100, who, server::CENTRAL);
    r.text("CLIENT", &c.client)
        .text("CMNTY", &c.community)
        .ch("NODETYPE", c.node_type)
        .text("NODENAME", &c.node);
    r.into_bytes()
}

/// `ZSCN0100`: a conversion map from `from` to `to` of `map_type` (1
/// round trip, 2 substitution, 3 best fit).
#[must_use]
pub fn zscn0100(who: &Who, from: i32, to: i32, map_type: i16) -> Vec<u8> {
    let mut r = header(&ZSCN0100, who, server::CENTRAL);
    r.bin4("FROMCCSID", from)
        .bin4("TOCCSID", to)
        .bin2("MAPTYPE", map_type);
    r.into_bytes()
}

/// `ZDAI0100`: the client's interface type, name and level.
#[must_use]
pub fn zdai0100(who: &Who, interface: (&str, &str, &str)) -> Vec<u8> {
    let mut r = header(&ZDAI0100, who, server::SQL);
    r.text("IFCTYPE", interface.0)
        .text("IFCNAME", interface.1)
        .text("IFCLEVEL", interface.2);
    r.into_bytes()
}

/// A native database request (`ZDAD0100`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DbFile {
    /// File, library, member.
    pub file: (String, String, String),
    /// Authority (`*USE`, ...).
    pub authority: String,
    /// Based-on file and library.
    pub base: (String, String),
    /// Override file, library and member.
    pub ovr: (String, String, String),
}

/// `ZDAD0100`.
#[must_use]
pub fn zdad0100(who: &Who, d: &DbFile) -> Vec<u8> {
    let mut r = header(&ZDAD0100, who, server::NDB);
    r.text("FILE", &d.file.0)
        .text("LIB", &d.file.1)
        .text("MBR", &d.file.2)
        .text("AUT", &d.authority)
        .text("BASEFILE", &d.base.0)
        .text("BASELIB", &d.base.1)
        .text("OVRFILE", &d.ovr.0)
        .text("OVRLIB", &d.ovr.1)
        .text("OVRMBR", &d.ovr.2);
    r.into_bytes()
}

/// `ZDAD0200`: the libraries being added.
#[must_use]
pub fn zdad0200(who: &Who, libs: &[String]) -> Vec<u8> {
    let mut r = header(&ZDAD0200, who, server::NDB);
    r.bin4("NBRLIBS", n32(libs.len()));
    let mut b = r.into_bytes();
    for l in libs {
        b.extend(text::fixed(l, 10));
    }
    b
}

/// An object information request (`ZDAR0100`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DbObject {
    /// Library, relational database, package, file, member, format.
    pub lib: String,
    /// Relational database.
    pub rdb: String,
    /// Package.
    pub package: String,
    /// File.
    pub file: String,
    /// Member.
    pub member: String,
    /// Record format.
    pub format: String,
}

/// `ZDAR0100`.
#[must_use]
pub fn zdar0100(who: &Who, o: &DbObject) -> Vec<u8> {
    let mut r = header(&ZDAR0100, who, server::ROI);
    r.text("LIB", &o.lib)
        .text("RDB", &o.rdb)
        .text("PKG", &o.package)
        .text("FILE", &o.file)
        .text("MBR", &o.member)
        .text("FMT", &o.format);
    r.into_bytes()
}

/// `ZDAR0200`: primary key and foreign key tables.
#[must_use]
pub fn zdar0200(who: &Who, pk: (&str, &str), fk: (&str, &str)) -> Vec<u8> {
    let mut r = header(&ZDAR0200, who, server::ROI);
    r.text("PKLIB", pk.0)
        .text("PKNAME", pk.1)
        .text("FKLIB", fk.0)
        .text("FKNAME", fk.1);
    r.into_bytes()
}

/// An SQL request (`ZDAQ0100`, `ZDAQ0200`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sql {
    /// Statement name.
    pub statement: String,
    /// Cursor name.
    pub cursor: String,
    /// Prepare option.
    pub prepare_option: String,
    /// Open attributes.
    pub open_attr: String,
    /// Package and its library.
    pub package: (String, String),
    /// DRDA connect indicator.
    pub drda: i16,
    /// Commit indicator.
    pub commit: char,
    /// The statement text.
    pub text: String,
    /// Extended cursor name.
    pub ext_cursor: String,
    /// Default SQL collection (`ZDAQ0200` only).
    pub default_lib: String,
}

fn sql_common(r: &mut Rec, s: &Sql) {
    r.text("STMTNAME", &s.statement)
        .text("CURSOR", &s.cursor)
        .text("PREPOPT", &s.prepare_option)
        .text("OPENATR", &s.open_attr)
        .text("PKG", &s.package.0)
        .text("PKGLIB", &s.package.1)
        .bin2("DRDA", s.drda)
        .ch("COMMIT", s.commit);
}

/// `ZDAQ0100`: the text cut at 512.
#[must_use]
pub fn zdaq0100(who: &Who, s: &Sql) -> Vec<u8> {
    let mut r = header(&ZDAQ0100, who, server::SQL);
    sql_common(&mut r, s);
    r.text("SQLTEXT", &s.text).text("EXTCURSOR", &s.ext_cursor);
    r.into_bytes()
}

/// `ZDAQ0200`: the whole text, then the extended cursor name.
#[must_use]
pub fn zdaq0200(who: &Who, s: &Sql) -> Vec<u8> {
    let sql = text::ebcdic(&s.text);
    let cur = text::ebcdic(&s.ext_cursor);
    let mut r = header(&ZDAQ0200, who, server::SQL);
    sql_common(&mut r, s);
    r.text("DFTLIB", &s.default_lib)
        .bytes("RSV1", &[])
        .bin4(
            "OFFEXTCUR",
            if cur.is_empty() {
                0
            } else {
                n32(ZDAQ0200.size + sql.len())
            },
        )
        .bin4("LENEXTCUR", n32(cur.len()))
        .bytes("RSV2", &[])
        .bin4("SQLLEN", n32(sql.len()));
    let mut b = r.into_bytes();
    b.extend(sql);
    b.extend(cur);
    b
}

/// A data queue request (`ZHQ00100`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataQueue {
    /// Queue and library.
    pub queue: (String, String),
    /// Key operator (`EQ`, `GE` ...; blank for none).
    pub key_op: String,
    /// The key.
    pub key: Vec<u8>,
}

/// `ZHQ00100`.
#[must_use]
pub fn zhq00100(who: &Who, q: &DataQueue) -> Vec<u8> {
    let mut r = header(&ZHQ00100, who, server::DATAQ);
    r.text("OBJ", &q.queue.0)
        .text("LIB", &q.queue.1)
        .text("KEYOP", &q.key_op)
        .bin4("KEYLEN", n32(q.key.len()))
        .bytes_blank("KEY", &q.key);
    r.into_bytes()
}

/// Access the file server request asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // IBM's four flags
pub struct Access {
    /// Read.
    pub read: bool,
    /// Write.
    pub write: bool,
    /// Read and write.
    pub read_write: bool,
    /// Delete allowed.
    pub delete: bool,
}

fn flag(b: bool) -> char {
    if b { '1' } else { '0' }
}

fn pwfs_head(l: &Layout, who: &Who, a: Access) -> Rec {
    let mut r = l.blank();
    r.text("USRPRF", &who.user)
        .text("SRVID", server::FILE)
        .bin4("FUNCID", who.function)
        .text("FMTNAME", l.name)
        .ch("READ", flag(a.read))
        .ch("WRITE", flag(a.write))
        .ch("READWRITE", flag(a.read_write))
        .ch("DELETE", flag(a.delete));
    r
}

/// `PWFS0100`: the file name follows, in the job's CCSID.
#[must_use]
pub fn pwfs0100(who: &Who, a: Access, name: &str) -> Vec<u8> {
    let n = text::ebcdic(name);
    let mut r = pwfs_head(&PWFS0100, who, a);
    r.bin4("NAMELEN", n32(n.len()));
    let mut b = r.into_bytes();
    b.extend(n);
    b
}

/// `PWFS0200`: the object type created, the source and the target.
#[must_use]
pub fn pwfs0200(who: &Who, a: Access, object_type: &str, source: &str, target: &str) -> Vec<u8> {
    let s = text::ebcdic(source);
    let t = text::ebcdic(target);
    let mut r = pwfs_head(&PWFS0200, who, a);
    let s_off = PWFS0200.size;
    let t_off = s_off + s.len();
    r.text("OBJTYPE", object_type)
        .bytes("RSV1", &[])
        .bin4("SRCOFF", n32(s_off))
        .bin4("SRCLEN", n32(s.len()))
        .bin4("TGTOFF", if t.is_empty() { 0 } else { n32(t_off) })
        .bin4("TGTLEN", n32(t.len()))
        .bytes("RSV2", &[]);
    let mut b = r.into_bytes();
    b.extend(s);
    b.extend(t);
    b
}

/// `ENTR0100`.
#[must_use]
pub fn entr0100(who: &Who) -> Vec<u8> {
    header(&ENTR0100, who, server::NETPRT).into_bytes()
}

/// A spooled file's identity (`Qnps_Splf_ID`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SplfId {
    /// Job name, user, number.
    pub job: (String, String, String),
    /// Spooled file name.
    pub name: String,
    /// Spooled file number.
    pub number: i32,
    /// Job system name.
    pub system: String,
    /// Creation date `CYYMMDD` and time `HHMMSS`.
    pub created: (String, String),
}

/// `SPLF0100`: the spooled file, then the request's data.
#[must_use]
pub fn splf0100(who: &Who, s: &SplfId, data: &[u8]) -> Vec<u8> {
    let mut r = header(&SPLF0100, who, server::NETPRT);
    r.text("JOBNAME", &s.job.0)
        .text("JOBUSER", &s.job.1)
        .text("JOBNBR", &s.job.2)
        .text("SPLFNAME", &s.name)
        .bin4("SPLFNBR", s.number)
        .text("JOBSYS", &s.system)
        .text("CRTDATE", &s.created.0)
        .text("CRTTIME", &s.created.1)
        .bin4("DATALEN", n32(data.len()));
    let mut b = r.into_bytes();
    b.extend(data);
    b
}

/// What a remote command server request runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Remote {
    /// A CL command: its text (in `ccsid`).
    Command {
        /// The command's CCSID.
        ccsid: i32,
        /// The command, already in that CCSID.
        text: Vec<u8>,
    },
    /// A program call: the program and its parameter descriptions as
    /// the server passes them.
    Program {
        /// Program and library.
        program: (String, String),
        /// Parameter information.
        parms: Vec<u8>,
    },
}

/// `CZRC0100`, padded to IBM's declared size.
#[must_use]
pub fn czrc0100(user: &str, r: &Remote) -> Vec<u8> {
    let (function, other) = match r {
        Remote::Command { text, .. } => (func::RMT_COMMAND, text.as_slice()),
        Remote::Program { parms, .. } => (func::RMT_PROGRAM_CALL, parms.as_slice()),
    };
    let who = Who {
        user: user.into(),
        function,
    };
    let mut rec = header(&CZRC0100, &who, server::RMTCMD);
    match r {
        Remote::Command { ccsid, .. } => {
            let mut obj = ccsid.to_be_bytes().to_vec();
            obj.extend([0x40; 6]);
            rec.bytes("OBJ", &obj).text("LIB", "");
        }
        Remote::Program { program, .. } => {
            rec.text("OBJ", &program.0).text("LIB", &program.1);
        }
    }
    rec.bin4("LENOTHER", n32(other.len()));
    let mut b = rec.into_bytes();
    b.extend(other);
    if b.len() < CZRC0100_DECLARED {
        b.resize(CZRC0100_DECLARED, 0x40);
    }
    b
}
