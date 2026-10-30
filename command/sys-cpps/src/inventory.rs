// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The inventory: for every command of `cmds/`, where its processing
//! program runs and what it is called. `inventory.toml` holds what the
//! defaults do not say; everything else is derived from the name.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

/// The inventory's source.
pub const INVENTORY: &str = include_str!("../inventory.toml");

/// Where a command's processing program runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Site {
    /// In the storage service, over the store (most commands).
    #[default]
    Service,
    /// In the job process: calls, the debugger, the job's own displays.
    Job,
    /// A stub: the processing program sends the message named and
    /// does nothing (a later phase's function).
    Stub,
}

/// One command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The command name.
    pub name: String,
    /// Where it runs.
    pub site: Site,
    /// The processing program, in `QSYS`.
    pub cpp: String,
    /// The completion message the processor sends when it has no
    /// better one, with the parameters whose values are its data.
    pub comp: Option<(String, Vec<String>)>,
    /// For a stub: the message it sends.
    pub stub: Option<String>,
    /// The console word of the Phase 2 to 6 library function the
    /// bridge calls; the command name in lower case by default.
    pub word: String,
    /// A note for the inventory table (`PHASE7.md`).
    pub note: String,
    /// `ALWLMTUSR(*YES)`: a limited user may type it.
    pub alwlmtusr: bool,
}

#[derive(Deserialize, Default)]
struct Over {
    #[serde(default)]
    site: Option<Site>,
    #[serde(default)]
    cpp: Option<String>,
    #[serde(default)]
    comp: Option<String>,
    #[serde(default)]
    comp_data: Vec<String>,
    #[serde(default)]
    stub: Option<String>,
    #[serde(default)]
    word: Option<String>,
    #[serde(default)]
    note: String,
    #[serde(default)]
    alwlmtusr: bool,
}

#[derive(Deserialize)]
struct Table {
    #[serde(default)]
    cmd: HashMap<String, Over>,
}

fn overrides() -> &'static HashMap<String, Over> {
    static T: OnceLock<HashMap<String, Over>> = OnceLock::new();
    T.get_or_init(|| {
        let t: Table = toml::from_str(INVENTORY).expect("inventory.toml parses");
        t.cmd
    })
}

/// The processing program's name for a command without one in the
/// inventory: `QC` and the first eight characters of the name.
#[must_use]
pub fn default_cpp(name: &str) -> String {
    let head: String = name.chars().take(8).collect();
    format!("QC{head}")
}

/// Every command, in name order.
pub fn all() -> &'static [Entry] {
    static T: OnceLock<Vec<Entry>> = OnceLock::new();
    T.get_or_init(|| {
        let over = overrides();
        crate::defs::DEFS
            .iter()
            .map(|(name, _)| {
                let o = over.get(*name);
                let site = o.and_then(|o| o.site).unwrap_or_default();
                Entry {
                    name: (*name).to_string(),
                    site: if o.and_then(|o| o.stub.as_ref()).is_some() {
                        Site::Stub
                    } else {
                        site
                    },
                    cpp: o
                        .and_then(|o| o.cpp.clone())
                        .unwrap_or_else(|| default_cpp(name)),
                    comp: o.and_then(|o| o.comp.clone().map(|c| (c, o.comp_data.clone()))),
                    stub: o.and_then(|o| o.stub.clone()),
                    word: o
                        .and_then(|o| o.word.clone())
                        .unwrap_or_else(|| name.to_ascii_lowercase()),
                    note: o.map(|o| o.note.clone()).unwrap_or_default(),
                    alwlmtusr: o.is_some_and(|o| o.alwlmtusr),
                }
            })
            .collect()
    })
}

/// A command's entry.
#[must_use]
pub fn entry(name: &str) -> Option<&'static Entry> {
    all().iter().find(|e| e.name == name)
}

/// The command a processing program name belongs to.
#[must_use]
pub fn by_cpp(cpp: &str) -> Option<&'static Entry> {
    all().iter().find(|e| e.cpp == cpp)
}

/// Overrides the inventory names for commands that have no definition
/// source (a test catches them).
#[must_use]
pub fn orphans() -> Vec<String> {
    overrides()
        .keys()
        .filter(|k| crate::defs::source(k).is_none())
        .cloned()
        .collect()
}

/// The commands `lic/slic/src/cpp/direct.rs` implements on the library
/// functions directly (kept in step with its match by hand).
pub const DIRECT: &[&str] = &[
    // --- IFS commands (lic/slic/src/cpp/ifscmd.rs)
    "ADDLNK",
    "ADDMFS",
    "CD",
    "CHGATR",
    "CHGAUT",
    "CHGCURDIR",
    "CHGOWN",
    "CHGPGP",
    "CHKIN",
    "CHKOUT",
    "CPY",
    "CRTDIR",
    "CRTUDFS",
    "DEL",
    "DLTUDFS",
    "DSPAUT",
    "DSPF",
    "DSPLNK",
    "DSPUDFS",
    "ENDJRN",
    "MD",
    "MKDIR",
    "MOUNT",
    "MOV",
    "RCLLNK",
    "RD",
    "REN",
    "RMDIR",
    "RMVLNK",
    "RMVMFS",
    "RNM",
    "SAV",
    "STRJRN",
    "UNMOUNT",
    "WRKAUT",
    "WRKLNK",
    // --- IFS stream file copies (lic/slic/src/cpp/stmfcmd.rs)
    "CPYFRMSTMF",
    "CPYTOSTMF",
    "ADDMSGD",
    "CRTMNU",
    "CRTPNLGRP",
    "DLTMNU",
    "DLTPNLGRP",
    "ALCOBJ",
    "CHGCMD",
    "CHGCMDDFT",
    "CHGDEVD",
    "CHGDTAARA",
    "CHGLF",
    "CHGLIB",
    "CHGMSGD",
    "CHGMSGF",
    "CHGNETA",
    "CHGOBJD",
    "CHGPF",
    "CHGPFM",
    "CHGPRXCMD",
    "CHGSRCPF",
    "CHGSYSVAL",
    "CLRLIB",
    "CPYSRCF",
    "CRTBNDCL",
    "CRTCLMOD",
    "CRTCLPGM",
    "CRTBNDRPG",
    "CRTRPGMOD",
    "CRTSQLRPGI",
    "CRTCMD",
    "CRTDTAARA",
    "CRTDTAQ",
    "CRTLIB",
    "CRTMSGF",
    "CRTPRXCMD",
    "DLCOBJ",
    "DLTAUTL",
    "DLTBNDDIR",
    "DLTCMD",
    "DLTDTAARA",
    "DLTDTAQ",
    "DLTLIB",
    "DLTMOD",
    "DLTMSGF",
    "DLTPGM",
    "DLTPRXCMD",
    "DLTSRVPGM",
    "CRTDEVDSP",
    "DLTDEVD",
    "CRTLINETH",
    "CHGLINETH",
    "DLTLIND",
    "DSPLIND",
    "WRKLIND",
    "DSPCMD",
    "DSPDEVD",
    "DSPDTAARA",
    "DSPLIB",
    "DSPNETA",
    "DSPOBJD",
    "DSPOVR",
    "DSPPTF",
    "DSPSFWRSC",
    "DSPSYSVAL",
    "GO",
    "INZPFM",
    "MOVOBJ",
    "MRGMSGF",
    "RCLSTG",
    "RCLTMPSTG",
    "RMVBNDDIRE",
    "RMVMSGD",
    "RNMM",
    "RNMOBJ",
    "RTVCLSRC",
    "RUNSQL",
    "RUNSQLSTM",
    "SNDJRNE",
    "UPDSRVPGM",
    "VRYCFG",
    "WRKCFGSTS",
    "WRKJRN",
    "WRKJRNA",
    "WRKLIB",
    "WRKOBJ",
    "WRKOBJLCK",
    "WRKSYSVAL",
    // The registration facility (lic/slic/src/stgsvc/regfac.rs).
    "ADDEXITPGM",
    "RMVEXITPGM",
    "WRKREGINF",
    // --- TCP/IP configuration (lic/slic/src/cpp/tcpcmd.rs)
    "ADDTCPHTE",
    "ADDTCPIFC",
    "ADDTCPPORT",
    "ADDTCPRTE",
    "ADDTCPSVR",
    "CHGTCPA",
    "CHGTCPDMN",
    "CHGTCPHTE",
    "CHGTCPIFC",
    "CHGTCPRTE",
    "CHGTCPSVR",
    "CPYTCPHT",
    "MRGTCPHT",
    "RMVTCPHTE",
    "RMVTCPIFC",
    "RMVTCPPORT",
    "RMVTCPRTE",
    "RMVTCPSVR",
    "RNMTCPHTE",
    "STRTCPIFC",
    "ENDTCPIFC",
    "ADDSRVTBLE",
    "RMVSRVTBLE",
    "ADDPCLTBLE",
    "RMVPCLTBLE",
    "ADDNETTBLE",
    "RMVNETTBLE",
    // --- the communications trace (lic/slic/src/cpp/cmntrc.rs)
    "STRCMNTRC",
    "ENDCMNTRC",
    "DLTCMNTRC",
    "CHKCMNTRC",
    "PRTCMNTRC",
];

/// Whether a job-site command also runs in the service: a display the
/// job shows as a panel, whose text the service answers for a job
/// without one (batch, `OUTPUT(*PRINT)`).
#[must_use]
pub fn in_service_too(name: &str) -> bool {
    JOB_SITE.contains(&name)
        && (DIRECT.contains(&name)
            || crate::entry(name).is_some_and(|e| BRIDGE_WORDS.contains(&e.word.as_str())))
}

/// The commands `work/qsjob` runs in the job (`command/jobsite.rs` and
/// the job words it bridges to).
pub const JOB_SITE: &[&str] = &[
    // --- panels: work management (work/qsjob/src/panels/work.rs)
    "WRKACTJOB",
    "DSPJOB",
    "WRKUSRJOB",
    "WRKSBSJOB",
    "WRKSBS",
    "DSPSBSD",
    "WRKJOBD",
    "DSPJOBD",
    "WRKCLS",
    "DSPCLS",
    "WRKSPLF",
    "WRKOUTQ",
    "WRKJOBQ",
    "WRKWTR",
    "WRKJOBSCDE",
    // --- panels: messages and objects (panels/objects.rs)
    "DSPDBR",
    "DSPDTAARA",
    "DSPFD",
    "DSPFFD",
    "DSPLIB",
    "DSPLIBL",
    "DSPLOG",
    "DSPMSG",
    "DSPMSGD",
    "DSPOBJAUT",
    "DSPOBJD",
    "WRKBNDDIR",
    "WRKMSG",
    "WRKOBJ",
    "WRKOBJOWN",
    "WRKRPYLE",
    // --- panels: system and security (panels/system.rs)
    "DSPSYSVAL",
    "WRKSYSVAL",
    "DSPNETA",
    "DSPSFWRSC",
    "DSPUSRPRF",
    "WRKUSRPRF",
    "DSPAUTUSR",
    "WRKCFGSTS",
    "DSPDEVD",
    "DSPAUTL",
    "WRKAUTL",
    "DSPSECAUD",
    "WRKREGINF",
    // --- system displays (work/qsjob/src/sysdsp/)
    "CHGGRPA",
    "DSPWSUSR",
    "ENDGRPJOB",
    "SETATNPGM",
    "TFRGRPJOB",
    "TFRSECJOB",
    // --- UIM APIs (work/qsjob/src/command/uiapi.rs)
    "CRTSCHIDX",
    "ADDSCHIDXE",
    // --- panels batch 2: work, spool, journals (panels/work2.rs)
    "WRKSBMJOB",
    "WRKSHRPOOL",
    "WRKSYSSTS",
    "WRKDSKSTS",
    "WRKSYSACT",
    "WRKOUTQD",
    "DSPSPLF",
    "WRKLIB",
    "WRKOBJLCK",
    "DSPPFM",
    "DSPJRN",
    "WRKJRN",
    "WRKJRNA",
    "DSPJRNRCVA",
    "DSPOVR",
    "WRKOBJPGP",
    "WRKSBSD",
    "DSPSAVF",
    "WRKPRB",
    // --- panels batch 2: programs, config, security, editors (panels/system2.rs)
    "DSPPGM",
    "DSPSRVPGM",
    "DSPMOD",
    "DSPBNDDIR",
    "DSPCMD",
    "DSPPGMREF",
    "WRKDEVD",
    "WRKHDWRSC",
    "DSPHDWRSC",
    "WRKLIND",
    "DSPLIND",
    "DSPPTF",
    "DSPAUTLOBJ",
    "DSPAUDJRNE",
    "EDTLIBL",
    "EDTOBJAUT",
    "EDTAUTL",
    // --- SEU (devtools/seu)
    "STRSEU",
    // --- PDM (devtools/pdm)
    "STRPDM",
    "WRKLIBPDM",
    "WRKOBJPDM",
    "WRKMBRPDM",
    "FNDSTRPDM",
    // --- SDA (devtools/sda)
    "STRSDA",
    // --- DFU and RLU (devtools/dfu)
    "STRDFU",
    "UPDDTA",
    "DSPDTA",
    "CHGDTA",
    "STRRLU",
    // --- STRDBG screens (devtools/dbg)
    "DSPMODSRC",
    "STRSRVJOB",
    "ENDSRVJOB",
    // --- STRSQL (devtools/isql)
    "STRSQL",
    // --- IFS panels (work/qsjob/src/panels/ifs.rs) and RTVCURDIR
    "RTVCURDIR",
    "WRKLNK",
    "DSPLNK",
    "WRKAUT",
    "DSPAUT",
    // --- EDTF and DSPF (devtools/edtf)
    "EDTF",
    "DSPF",
    // --- STRSST (devtools/sst)
    "STRSST",
    "ADDBKP",
    "CALL",
    "CHGDBG",
    "CHGPWD",
    "DLYJOB",
    "DSCJOB",
    "DSPBKP",
    "DSPJOBLOG",
    "ENDDBG",
    "ENDSYS",
    "PWRDWNSYS",
    "RCLACTGRP",
    "RCLRSC",
    "RMVBKP",
    "RTVCFGSTS",
    "RTVJOBA",
    "RTVMBRD",
    "RTVMSG",
    "RTVNETA",
    "RTVOBJD",
    "RTVSYSVAL",
    "RTVUSRPRF",
    "SBMJOB",
    "SIGNOFF",
    "STRDBG",
    "STRTCPSVR",
    "ENDTCPSVR",
    "STRHOSTSVR",
    "ENDHOSTSVR",
    "CHGTELNA",
    // --- SNTP, LPD, LPR and mail out (command/netcmd.rs)
    "CHGNTPA",
    "CHGLPDA",
    "CHGSMTPA",
    "SNDTCPSPLF",
    "LPR",
    "SNDSMTPEMM",
    // --- FTP (command/ftp.rs, net/ftp)
    "CHGFTPA",
    "FTP",
    "STRTCPFTP",
    // --- DDM and DRDA (command/ddm.rs)
    "ADDRDBDIRE",
    "CHGRDBDIRE",
    "RMVRDBDIRE",
    "WRKRDBDIRE",
    "CRTDDMF",
    "CHGDDMF",
    "CHGDDMTCPA",
    "STRSBS",
    "WRKJOB",
    // --- TCP/IP: STRTCP, ENDTCP and the CFGTCP menus (work/qsjob/src/command/tcp.rs)
    "STRTCP",
    "ENDTCP",
    "ENDTCPABN",
    "CFGTCP",
    "CFGTCPAPP",
    "CFGTCPBP",
    "CFGTCPRTD",
    "CFGTCPSNMP",
    "CFGTCPTELN",
    "CFGTCPSMTP",
    "CFGTCPPTP",
    "WRKSRVTBLE",
    "WRKPCLTBLE",
    "WRKNETTBLE",
    // --- the network tools (work/qsjob/src/command/nettools.rs; NETSTAT's
    // panels in work/qsjob/src/panels/tcp/netstat.rs)
    "NETSTAT",
    "PING",
    "VFYTCPCNN",
    "TRACEROUTE",
    "TRCTCPRTE",
];

/// The CL statements: defined so the analyzer and the prompter know
/// them, compiled by `cl`, never run as commands.
pub const CL_STATEMENTS: &[&str] = &[
    "CALLPRC",
    "CALLSUBR",
    "CHGVAR",
    "CLOSE",
    "DCL",
    "DCLF",
    "DCLPRCOPT",
    "DMPCLPGM",
    "DO",
    "DOFOR",
    "DOUNTIL",
    "DOWHILE",
    "ELSE",
    "ENDDO",
    "ENDPGM",
    "ENDRCV",
    "ENDSELECT",
    "ENDSUBR",
    "GOTO",
    "IF",
    "ITERATE",
    "LEAVE",
    "MONMSG",
    "OTHERWISE",
    "PGM",
    "RCVF",
    "RCVMSG",
    "RETURN",
    "RMVMSG",
    "RTNSUBR",
    "RTVDTAARA",
    "SELECT",
    "SNDF",
    "SNDPGMMSG",
    "SNDRCVF",
    "SNDUSRMSG",
    "SUBR",
    "TFRCTL",
    "WAIT",
    "WHEN",
];

/// The console words of the storage service's tables the bridge can
/// hand a keyword form to.
pub const BRIDGE_WORDS: &[&str] = &[
    "addaje",
    "addautle",
    "addbnddire",
    "addjobqe",
    "addjobscde",
    "addlible",
    "addpfm",
    "addpje",
    "addrpyle",
    "addrtge",
    "addwse",
    "anzdftpwd",
    "anzprfact",
    "apyjrnchg",
    "asp",
    "chgautle",
    "chgcls",
    "chgcurlib",
    "chgjob",
    "chgjobd",
    "chgjobq",
    "chgjobqe",
    "chgjobscde",
    "clof",
    "chgjrn",
    "chglibl",
    "chgmsgq",
    "chgobjaud",
    "chgobjown",
    "chgobjpgp",
    "chgdevprt",
    "chgoutq",
    "chgpje",
    "chgprtf",
    "chgrpyle",
    "chgrtge",
    "chgsbsd",
    "chgsecaud",
    "chgshrpool",
    "chgsplfa",
    "chgsyslibl",
    "chgusraud",
    "chgusrprf",
    "chgwtr",
    "chkobj",
    "chkstg",
    "clrjobq",
    "clrmsgq",
    "clroutq",
    "clrpfm",
    "commit",
    "cpyaudjrne",
    "cpyf",
    "cpyfrmimpf",
    "cpysplf",
    "cpytoimpf",
    "crtautl",
    "crtbnddir",
    "crtcls",
    "crtjobd",
    "crtjobq",
    "crtjrn",
    "crtjrnrcv",
    "crtlf",
    "crtmsgq",
    "crtoutq",
    "crtpf",
    "crtpgm",
    "crtdevprt",
    "crtdspf",
    "crtprtf",
    "crtsbsd",
    "crtsrcpf",
    "crtsrvpgm",
    "crtwscst",
    "crtusrprf",
    "curlib",
    "dbbench",
    "dltcls",
    "dltf",
    "dltjobd",
    "dltjobq",
    "dltjrn",
    "dltjrnrcv",
    "dltmsgq",
    "dltoutq",
    "dltovr",
    "dltsbsd",
    "dltsplf",
    "dltusrprf",
    "dspaudjrne",
    "dspautl",
    "dspautlobj",
    "dspautusr",
    "dspbnddir",
    "dspcls",
    "dspdbr",
    "dspfd",
    "dspffd",
    "dspjob",
    "dspjobd",
    "dspjrn",
    "dspjrnrcva",
    "dsplibl",
    "dsplog",
    "dspmod",
    "dspmsg",
    "dspmsgd",
    "dspobjaut",
    "dsppfm",
    "dsppgm",
    "dsppgmref",
    "dspsbsd",
    "dspsecaud",
    "dspsplf",
    "dspsrvpgm",
    "dspusrprf",
    "dtaara",
    "dtaq",
    "edtobjaut",
    "endcmtctl",
    "endjob",
    "endjrnpf",
    "endsbs",
    "endwtr",
    "grtobjaut",
    "grtusraut",
    "hldjob",
    "hldjobq",
    "hldjobscde",
    "hldoutq",
    "hldsplf",
    "hldwtr",
    "hw",
    "lib",
    "mirror",
    "naming",
    "obj",
    "opnqryf",
    "ovrdbf",
    "ovrmsgf",
    "ovrdspf",
    "ovrprtf",
    "prtadpobj",
    "prtcapture",
    "prtjobdaut",
    "prtpubaut",
    "prtpvtaut",
    "prtqaut",
    "prtsbsdaut",
    "prtsysseca",
    "prtusrobj",
    "prtusrprf",
    "qir",
    "rclstg",
    "rcvmsg",
    "rgzpfm",
    "rla",
    "rlsjob",
    "rlsjobq",
    "rlsjobscde",
    "rlsoutq",
    "rlssplf",
    "rlswtr",
    "rmvaje",
    "rmvautle",
    "rmvjobqe",
    "rmvjobscde",
    "rmvjrnchg",
    "rmvlible",
    "rmvm",
    "rmvmsg",
    "rmvpje",
    "rmvrpyle",
    "rmvrtge",
    "rmvwse",
    "rollbck",
    "rtvwscst",
    "rvkobjaut",
    "sndbrkmsg",
    "sndmsg",
    "sndrpymsg",
    "sql",
    "sqlrun",
    "srcget",
    "srcput",
    "strcmtctl",
    "strjrnpf",
    "strprtwtr",
    "strrmtwtr",
    "sysval",
    "txn",
    "unit",
    "updpgm",
    "usrspc",
    "whoami",
    "wmstats",
    "wrkactjob",
    "wrkdsksts",
    "wrkjobq",
    "wrkjobscde",
    "wrkmsgd",
    "wrkobjown",
    "wrkoutq",
    "wrkoutqd",
    "wrkrpyle",
    "wrksbs",
    "wrksbsjob",
    "wrkshrpool",
    "wrksplf",
    "wrksplfa",
    "wrksysact",
    "wrksyssts",
    "wrkusrprf",
    "wrkwtr",
];

/// Defined commands with no processing program in this release: the
/// analyzer accepts them and the service answers `CPF0006`
/// (`docs/PHASE7.md` lists them under Blocked).
pub const UNSUPPORTED: &[&str] = &[
    "CPYLIB",
    "POSDBF",
    "TFRJOB",
];
