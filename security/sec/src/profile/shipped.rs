// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped profiles, with IBM's names, classes and special
//! authorities (programs and tools look for them) and Quartz's text;
//! and the first IPL with security: profiles created, the existing
//! objects re-owned per IBM's conventions, the profile indexes built.
//! The attributes are an IBM i 7.1's (`tests/shipped_csv.rs` checks
//! them against the capture).

use store::obj::header::{Authority, text};
use store::obj::{ObjectId, auth, ops::read_header, types};
use store::txn::Txn;

use crate::profile::index;
use crate::profile::objects;
use crate::profile::payload::{LmtCpb, Tri, UsrPrf};
use crate::profile::spcaut::{SpecialAuth as S, UserClass as C};
use crate::rec::text_of;
use crate::{Result, password};

/// One shipped profile: what `DSPUSRPRF` of an IBM i 7.1 shows for it
/// (`tools/conformance/oracle/sec/expected/usrprf.csv`), the text ours.
#[derive(Debug, Clone, Copy)]
pub struct Shipped {
    /// Name.
    pub name: &'static str,
    /// User class.
    pub class: C,
    /// Special authorities.
    pub spcaut: S,
    /// Text, Quartz-written.
    pub text: &'static str,
    /// `PTYLMT`.
    pub ptylmt: u8,
    /// `JOBD`: name and library.
    pub jobd: (&'static str, &'static str),
    /// `MSGQ`: name and library; `None` is the profile's own in `QUSRSYS`.
    pub msgq: Option<(&'static str, &'static str)>,
    /// `INLPGM`: name and library.
    pub inlpgm: (&'static str, &'static str),
    /// `INLMNU`: name and library.
    pub inlmnu: (&'static str, &'static str),
    /// `CURLIB`.
    pub curlib: &'static str,
    /// `DLVRY`.
    pub dlvry: &'static str,
    /// `SEV`.
    pub sev: u8,
    /// `SPCENV`.
    pub spcenv: &'static str,
    /// `ATNPGM`: name and library.
    pub atnpgm: (&'static str, &'static str),
    /// `ASTLVL`.
    pub astlvl: &'static str,
    /// `LMTCPB`.
    pub lmtcpb: LmtCpb,
    /// `STATUS(*DISABLED)`.
    pub disabled: bool,
    /// `DSPSGNINF(*SYSVAL) LMTDEVSSN(*SYSVAL)`; `false` is `*NO` for both.
    pub signon_info: bool,
    /// `ACGCDE`.
    pub acgcde: &'static str,
    /// `OUTQ`: name and library.
    pub outq: (&'static str, &'static str),
    /// `PWDEXPITV(*NOMAX)` rather than `*SYSVAL`.
    pub pwdexpitv_nomax: bool,
}

const fn p(name: &'static str, class: C, spcaut: S, text: &'static str) -> Shipped {
    Shipped {
        name,
        class,
        spcaut,
        text,
        ptylmt: 0,
        jobd: ("QDFTJOBD", "QGPL"),
        msgq: None,
        inlpgm: ("*NONE", ""),
        inlmnu: ("MAIN", "*LIBL"),
        curlib: "*CRTDFT",
        dlvry: "*NOTIFY",
        sev: 0,
        spcenv: "*SYSVAL",
        atnpgm: ("*NONE", ""),
        astlvl: "*SYSVAL",
        lmtcpb: LmtCpb::No,
        disabled: false,
        signon_info: true,
        acgcde: "*SYS",
        outq: ("*WRKSTN", ""),
        pwdexpitv_nomax: false,
    }
}

impl Shipped {
    const fn ptylmt(mut self, n: u8) -> Self {
        self.ptylmt = n;
        self
    }
    const fn jobd(mut self, name: &'static str, lib: &'static str) -> Self {
        self.jobd = (name, lib);
        self
    }
    const fn msgq(mut self, name: &'static str, lib: &'static str) -> Self {
        self.msgq = Some((name, lib));
        self
    }
    const fn inlpgm(mut self, name: &'static str, lib: &'static str) -> Self {
        self.inlpgm = (name, lib);
        self
    }
    const fn inlmnu(mut self, name: &'static str, lib: &'static str) -> Self {
        self.inlmnu = (name, lib);
        self
    }
    const fn curlib(mut self, lib: &'static str) -> Self {
        self.curlib = lib;
        self
    }
    const fn dlvry(mut self, d: &'static str) -> Self {
        self.dlvry = d;
        self
    }
    const fn sev(mut self, n: u8) -> Self {
        self.sev = n;
        self
    }
    const fn spcenv(mut self, e: &'static str) -> Self {
        self.spcenv = e;
        self
    }
    const fn atnpgm(mut self, name: &'static str, lib: &'static str) -> Self {
        self.atnpgm = (name, lib);
        self
    }
    const fn astlvl(mut self, a: &'static str) -> Self {
        self.astlvl = a;
        self
    }
    const fn lmtcpb(mut self, l: LmtCpb) -> Self {
        self.lmtcpb = l;
        self
    }
    const fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
    const fn no_signon_info(mut self) -> Self {
        self.signon_info = false;
        self
    }
    const fn acgcde(mut self, a: &'static str) -> Self {
        self.acgcde = a;
        self
    }
    const fn outq(mut self, name: &'static str, lib: &'static str) -> Self {
        self.outq = (name, lib);
        self
    }
    const fn pwdexpitv_nomax(mut self) -> Self {
        self.pwdexpitv_nomax = true;
        self
    }
}

const JS: S = S(S::JOBCTL.0 | S::SAVSYS.0);
const INSTALL: S = S(S::ALLOBJ.0 | S::IOSYSCFG.0 | S::JOBCTL.0 | S::SAVSYS.0 | S::SECADM.0);
const SERVICE: S = S(S::JOBCTL.0 | S::SERVICE.0);
const AGENT: S = S(S::IOSYSCFG.0 | S::JOBCTL.0 | S::SERVICE.0);
const PM: S = S(S::IOSYSCFG.0 | S::JOBCTL.0);

/// Every shipped profile, as IBM i 7.1 ships them (`QSOC` and `QUMB`
/// are older releases'). Only `QSECOFR` has a password.
pub static PROFILES: &[Shipped] = &[
    p("QSYS", C::SecOfr, S::ALL, "Owner of the operating system's objects"),
    p("QSECOFR", C::SecOfr, S::ALL, "Security officer").atnpgm("*SYSVAL", ""),
    p("QSYSOPR", C::SysOpr, JS, "System operator")
        .msgq("QSYSOPR", "QSYS")
        .inlmnu("SYSTEM", "*LIBL")
        .dlvry("*BREAK")
        .sev(40)
        .atnpgm("*SYSVAL", ""),
    p("QPGMR", C::Pgmr, JS, "Programmer")
        .ptylmt(3)
        .atnpgm("*SYSVAL", ""),
    p("QUSER", C::User, S::NONE, "Workstation user")
        .ptylmt(3)
        .atnpgm("*SYSVAL", ""),
    p("QSRV", C::Pgmr, SERVICE, "Service representative")
        .atnpgm("QSCATTN", "QSYS")
        .astlvl("*INTERMED"),
    p("QSRVBAS", C::Pgmr, S::JOBCTL, "Basic service representative")
        .atnpgm("QSCATTN", "QSYS")
        .astlvl("*INTERMED"),
    p("QDFTOWN", C::User, S::NONE, "Owner of objects whose owner was deleted").ptylmt(3),
    p("QSPL", C::User, S::NONE, "Spooling"),
    p("QSPLJOB", C::User, S::NONE, "Spooling jobs"),
    p("QTCP", C::SysOpr, S::JOBCTL, "TCP/IP")
        .curlib("QTCP")
        .spcenv("*NONE")
        .dlvry("*HOLD"),
    p("QDBSHR", C::User, S::NONE, "Database sharing"),
    p("QDBSHRDO", C::User, S::NONE, "Database sharing, data owner"),
    p("QDOC", C::User, S::NONE, "Documents"),
    p("QLPAUTO", C::SysOpr, INSTALL, "Licensed program automatic install")
        .inlpgm("QLPINATO", "QSYS")
        .msgq("QSYSOPR", "QSYS")
        .inlmnu("*SIGNOFF", "")
        .spcenv("*NONE")
        .dlvry("*HOLD")
        .sev(99),
    p("QLPINSTALL", C::SysOpr, INSTALL, "Licensed program install")
        .spcenv("*NONE")
        .dlvry("*HOLD"),
    p("QANZAGENT", C::SysOpr, S::NONE, "Trace analyzer agent server").no_signon_info(),
    p("QAUTPROF", C::User, S::NONE, "Authority profile"),
    p("QBRMS", C::User, S::NONE, "Backup and recovery"),
    p("QCLUMGT", C::User, S::NONE, "Cluster management")
        .spcenv("*NONE")
        .disabled(),
    p("QCLUSTER", C::User, S::IOSYSCFG, "Clustering").spcenv("*NONE"),
    p("QCOLSRV", C::User, S::NONE, "Collection services"),
    p("QDIRSRV", C::User, S::NONE, "Directory services")
        .jobd("QBATCH", "QGPL")
        .spcenv("*NONE")
        .lmtcpb(LmtCpb::Yes)
        .dlvry("*HOLD")
        .no_signon_info(),
    p("QDLFM", C::User, S::NONE, "Data link file manager"),
    p("QDSNX", C::User, S::NONE, "Distributed systems node executive").ptylmt(3),
    p("QEJB", C::User, S::NONE, "Enterprise beans"),
    p("QEJBSVR", C::User, S::NONE, "Application server")
        .spcenv("*NONE")
        .no_signon_info(),
    p("QFNC", C::User, S::NONE, "Finance").ptylmt(3),
    p("QGATE", C::User, S::NONE, "Gateway"),
    p("QIBMHELP", C::SysOpr, S::NONE, "Online help server").no_signon_info(),
    p("QIPP", C::User, S::NONE, "Internet printing"),
    p("QLWISVR", C::User, S::NONE, "Integrated application servers").no_signon_info(),
    p("QMGTC", C::User, S::NONE, "Management central")
        .jobd("QYPSJOBD", "QSYS")
        .spcenv("*NONE")
        .no_signon_info(),
    p("QMSF", C::User, S::NONE, "Mail server framework"),
    p("QNETSPLF", C::User, S::NONE, "Network spooled files")
        .spcenv("*NONE")
        .dlvry("*HOLD"),
    p("QNFSANON", C::User, S::NONE, "Network file system anonymous user"),
    p("QNTP", C::User, S::NONE, "Network time")
        .ptylmt(1)
        .jobd("QTOTNTP", "QSYS"),
    p("QPEX", C::User, S::NONE, "Performance explorer"),
    p("QPM400", C::User, PM, "Performance management"),
    p("QRJE", C::Pgmr, S::JOBCTL, "Remote job entry"),
    p("QSNADS", C::User, S::NONE, "SNA distribution services"),
    p("QSOC", C::User, S::NONE, "Communications"),
    p("QSRVAGT", C::SysOpr, AGENT, "Service agent")
        .outq("QS9SRVAGT", "QSRVAGT")
        .spcenv("*NONE")
        .no_signon_info(),
    p("QTCM", C::User, S::NONE, "Cluster resource services")
        .spcenv("*NONE")
        .disabled(),
    p("QTFTP", C::User, S::NONE, "Trivial file transfer"),
    p("QTMHHTP1", C::User, S::NONE, "Web server, CGI jobs").msgq("QTMHHTTP", "QUSRSYS"),
    p("QTMHHTTP", C::User, S::NONE, "Web server"),
    p("QTMPLPD", C::User, S::NONE, "Line printer daemon")
        .ptylmt(3)
        .acgcde("")
        .msgq("QTMPLPD", "QTCP")
        .atnpgm("*SYSVAL", "")
        .pwdexpitv_nomax(),
    p("QTSTRQS", C::User, S::NONE, "Test request"),
    p("QUMB", C::User, S::NONE, "Ultimedia"),
    p("QWEBADMIN", C::User, S::NONE, "Web administration").no_signon_info(),
    p("QWSERVICE", C::User, S::NONE, "Integrated web services").no_signon_info(),
    p("QYCMCIMOM", C::User, S::NONE, "CIM object manager"),
    p("QYPSJSVR", C::User, S::NONE, "Management central Java server"),
];

/// The shipped profile `name`, if it is one.
#[must_use]
pub fn shipped(name: &str) -> Option<&'static Shipped> {
    PROFILES.iter().find(|s| s.name == name)
}

/// The libraries the system owns.
pub const SYSTEM_LIBRARIES: &[&str] = &[
    "QSYS",
    "QGPL",
    "QUSRSYS",
    "QSPL",
    "QRECOVERY",
    "QHLPSYS",
    "QSYS2",
    "SYSIBM",
];

/// What [`install`] did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Installed {
    /// Profiles created.
    pub profiles: usize,
    /// Objects re-owned.
    pub reowned: usize,
    /// Index entries built.
    pub indexed: usize,
}

/// The profile a shipped entry starts as.
#[must_use]
pub fn profile_of(s: &Shipped, qsecurity: u8, pwdlvl: u8, now: u64) -> UsrPrf {
    let mut u = UsrPrf::new(s.name, qsecurity);
    let pair = |(n, l): (&str, &str)| (n.to_string(), l.to_string());
    u.usrcls = s.class;
    u.spcaut = s.spcaut;
    u.text = s.text.into();
    u.created = now;
    u.changed = now;
    u.ptylmt = s.ptylmt;
    u.jobd = pair(s.jobd);
    u.msgq = s.msgq.map_or_else(|| (s.name.to_string(), "QUSRSYS".to_string()), pair);
    u.inlpgm = pair(s.inlpgm);
    u.inlmnu = pair(s.inlmnu);
    u.curlib = s.curlib.into();
    u.dlvry = s.dlvry.into();
    u.sev = s.sev;
    u.spcenv = s.spcenv.into();
    u.atnpgm = pair(s.atnpgm);
    u.astlvl = s.astlvl.into();
    u.lmtcpb = s.lmtcpb;
    u.disabled = s.disabled;
    if !s.signon_info {
        u.dspsgninf = Tri::No;
        u.lmtdevssn = Tri::No;
    }
    u.acgcde = s.acgcde.into();
    u.outq = pair(s.outq);
    if s.pwdexpitv_nomax {
        u.pwdexpitv = crate::profile::payload::NOMAX;
    }
    u.locale = "*NONE".into();
    u.pwdchgblk = crate::profile::payload::NOMAX;
    if s.name == "QSECOFR" {
        u.pwd_none = false;
        u.pwdexp = true;
        u.hashes = password::hashes_for("QSECOFR", pwdlvl);
        u.pwd_changed = now;
    }
    u
}

/// Create the shipped profiles that are missing. The first time (no
/// `QSYS` profile yet) also re-own every object and build the indexes.
pub fn install(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    qsecurity: u8,
    pwdlvl: u8,
    now: u64,
) -> Result<Installed> {
    let first = !objects::exists(txn, qsys, "QSYS");
    let mut done = Installed::default();
    for s in PROFILES {
        if objects::exists(txn, qsys, s.name) {
            continue;
        }
        let u = profile_of(s, qsecurity, pwdlvl, now);
        objects::create(txn, qsys, &u, "QSYS", Authority::EXCLUDE, now)?;
        done.profiles += 1;
    }
    if first {
        done.reowned = reown(txn, qsys)?;
        done.indexed = index::rebuild(txn, qsys)?;
        // QLMTSECOFR: the security officer is authorized to the console.
        let lib = store::obj::library::Library::load(txn, qsys)?;
        let console = qsname::Name::parse("QSCONSOLE").expect("valid");
        if let Some(dev) = lib.lookup(txn, types::DEVD, console)? {
            crate::auth::grant::set_private(
                txn,
                qsys,
                dev,
                "QSECOFR",
                Some(crate::auth::bits::Auth::CHANGE),
            )?;
        }
    }
    Ok(done)
}

/// Re-own the objects that predate security: everything in a system
/// library, and the system libraries, to `QSYS`, with IBM's public
/// authorities; a user's message queue in `QUSRSYS` to its user;
/// anything else whose owner is not a profile to `QDFTOWN`.
pub fn reown(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<usize> {
    let mut n = 0;
    for id in index::all_objects(txn, qsys)? {
        let (page, mut h) = read_header(txn, id)?;
        let lib = if id == qsys {
            "QSYS".to_string()
        } else {
            read_header(txn, h.library)?.1.name.to_string()
        };
        let name = h.name.to_string();
        let system = if id.type_code == types::LIB {
            SYSTEM_LIBRARIES.contains(&name.as_str())
        } else {
            SYSTEM_LIBRARIES.contains(&lib.as_str())
        };
        let owner = text_of(&h.owner);
        let new_owner =
            if id.type_code == types::MSGQ && lib == "QUSRSYS" && objects::exists(txn, qsys, &name)
            {
                name.clone()
            } else if system {
                "QSYS".into()
            } else if objects::exists(txn, qsys, &owner) {
                owner.clone()
            } else {
                "QDFTOWN".into()
            };
        h.owner = text(&new_owner)?;
        if system {
            h.public_authority = public_for(id.type_code, &name);
        }
        auth::save_authority(txn, page, &mut h)?;
        n += 1;
    }
    Ok(n)
}

/// IBM's `*PUBLIC` for a system object of a type.
#[must_use]
pub fn public_for(type_code: u16, name: &str) -> Authority {
    match type_code {
        types::LIB => match name {
            "QGPL" => Authority::CHANGE,
            "QSPL" | "QRECOVERY" => Authority::EXCLUDE,
            _ => Authority::USE,
        },
        types::USRPRF | types::SYSVAL | types::SPLF => Authority::EXCLUDE,
        types::MSGQ => Authority::CHANGE,
        _ => Authority::USE,
    }
}
