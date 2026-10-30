// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NETSTAT panels (`QTOCNETS`) against the Phase 10 goldens of
//! pub400 (`golden/phase10/netstat`): each rendered with the values
//! the golden shows (its masks included) and compared row by row. One
//! test per golden, named after it.

mod tcpcommon;

use tcpcommon::{compare, shipped};
use uim::render::{ListState, MapDialog, Options, render};

fn list(rows: &[&[(&str, &str)]]) -> ListState {
    ListState {
        rows: rows
            .iter()
            .map(|r| r.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect())
            .collect(),
        top: 0,
        total: Some(rows.len()),
        complete: true,
    }
}

fn opts() -> Options {
    Options {
        system: "SYSNAME".into(),
        ..Options::default()
    }
}

fn view(n: usize) -> Options {
    Options { view: n, ..opts() }
}

/// The golden of `name` under `golden/phase10/netstat`.
fn golden(name: &str) -> String {
    format!("../netstat/{name}")
}

fn netstat() -> uim::PanelGroup {
    shipped("QTOCNETS")
}

fn data(d: &mut MapDialog, pairs: &[(&str, &str)]) {
    for (k, v) in pairs {
        d.set(k, v);
    }
}

#[test]
fn netstat_menu() {
    let s = render(&netstat(), "NETSTAT", &MapDialog::new(), &opts()).unwrap();
    compare(&golden("netstat"), &s);
}

fn ifcs() -> ListState {
    list(&[
        &[("IADR", "*IP4DHCP"), ("INET", "*"), ("ILIND", "LINETH"), ("ISTS", "Inactive"), ("IVLAN", "*NONE")],
        &[("IADR", "127.0.0.1"), ("INET", "127.0.0.0"), ("ILIND", "*LOOPBACK"), ("ISTS", "Active"), ("IVLAN", "*NONE")],
        &[("IADR", "185.113.5.134"), ("INET", "185.113.5.0"), ("ILIND", "LINETH"), ("ISTS", "Active"), ("IVLAN", "*NONE")],
    ])
}

#[test]
fn netstat_1_ifc() {
    let mut d = MapDialog::new();
    d.set_list("IFCS", ifcs());
    let s = render(&netstat(), "IFCSTSD", &d, &opts()).unwrap();
    compare(&golden("netstat-1-ifc"), &s);
}

#[test]
fn netstat_1_ifc_f11() {
    let mut d = MapDialog::new();
    d.set_list("IFCS", ifcs());
    d.set("VIEW", "2");
    let s = render(&netstat(), "IFCSTSD", &d, &view(1)).unwrap();
    compare(&golden("netstat-1-ifc-f11"), &s);
}

fn loopback_detail(d: &mut MapDialog) {
    data(
        d,
        &[
            ("DHOST", "LOOPBACK"),
            ("DADR", "127.0.0.1"),
            ("DMASK", "255.0.0.0"),
            ("DNET", "127.0.0.0"),
            ("DHOSTA", "0.0.0.1"),
            ("DBCAST", "*NONE"),
            ("DSTS", "Active"),
            ("DCHG", "MM/DD/YY  HH:MM:SS"),
            ("DLIND", "*LOOPBACK"),
            ("DVLAN", "*NONE"),
            ("DLTYPE", "*NONE"),
            ("DLCLIFC", "*NONE"),
            ("DTOS", "*NORMAL"),
            ("DMTU", "576"),
            ("DAUTO", "*YES"),
            ("DDHCP", "*NO"),
            ("DALIAS", "LOCALHOST"),
            ("DTEXT", "*NONE"),
        ],
    );
}

#[test]
fn netstat_1_ifc_5() {
    let mut d = MapDialog::new();
    loopback_detail(&mut d);
    let s = render(&netstat(), "DSPIFC", &d, &opts()).unwrap();
    compare(&golden("netstat-1-ifc-5"), &s);
    let o = Options { page: 1, ..opts() };
    let s = render(&netstat(), "DSPIFC", &d, &o).unwrap();
    compare(&golden("netstat-1-ifc-5-2"), &s);
}

fn routes() -> ListState {
    let r = |dest: &'static str, mask: &'static str, hop: &'static str, mtu: &'static str, ty: &'static str| -> Vec<(&'static str, &'static str)> {
        vec![
            ("RDEST", dest),
            ("RMASK", mask),
            ("RHOP", hop),
            ("RAVAIL", "  *YES"),
            ("RTOS", "*NORMAL"),
            ("RMTU", mtu),
            ("RTYPE", ty),
            ("RSRC", "*CFG"),
        ]
    };
    let rows = [
        r("127.0.0.0", "255.0.0.0", "*DIRECT", "576", "*DIRECT"),
        r("185.113.5.0", "255.255.255.0", "*DIRECT", "1500", "*DIRECT"),
        r("224.0.0.0", "240.0.0.0", "*DIRECT", "1500", "*DIRECT"),
        r("224.0.0.0", "240.0.0.0", "*DIRECT", "576", "*DIRECT"),
        r("*DFTROUTE", "*NONE", "185.113.5.253", "1500", "*DFTROUTE"),
    ];
    let refs: Vec<&[(&str, &str)]> = rows.iter().map(Vec::as_slice).collect();
    list(&refs)
}

#[test]
fn netstat_2_rte() {
    let mut d = MapDialog::new();
    d.set_list("RTES", routes());
    let s = render(&netstat(), "RTEINF", &d, &opts()).unwrap();
    compare(&golden("netstat-2-rte"), &s);
}

#[test]
fn netstat_2_rte_f11() {
    let mut d = MapDialog::new();
    d.set_list("RTES", routes());
    d.set("VIEW", "2");
    let s = render(&netstat(), "RTEINF", &d, &view(1)).unwrap();
    compare(&golden("netstat-2-rte-f11"), &s);
}

#[test]
fn netstat_2_rte_5() {
    let mut d = MapDialog::new();
    data(
        &mut d,
        &[
            ("DDEST", "*DFTROUTE"),
            ("DORDER", "1"),
            ("DHOP", "185.113.5.253"),
            ("DTOS", "*NORMAL"),
            ("DAVAIL", "*YES"),
            ("DTYPE", "*DFTROUTE"),
            ("DSRC", "*CFG"),
            ("DCHG", "MM/DD/YY  HH:MM:SS"),
            ("DMTU", "1500"),
            ("DDUP", "5"),
            ("DTEXT", "*NONE"),
        ],
    );
    let s = render(&netstat(), "DSPDFT", &d, &opts()).unwrap();
    compare(&golden("netstat-2-rte-5"), &s);
}

/// pub400's listening sockets as option 3 lists them, every view's values.
fn conns() -> ListState {
    let rows: Vec<Vec<(&str, &str)>> = [
        ("ftp-con >", "000:HH:MM", "Listen", "QTCP", "0", "0", "21", "*TCP"),
        ("telnet", "000:HH:MM", "Listen", "QTCP", "0", "0", "23", "*TCP"),
        ("www-http", "000:HH:MM", "Listen", "QTMHHTTP", "0", "0", "80", "*TCP"),
        ("ntp", "000:HH:MM", "*UDP", "QNTP", "15312", "27565", "123", "*UDP"),
        ("https", "000:HH:MM", "Listen", "QTMHHTTP", "0", "0", "443", "*TCP"),
        ("drda", "000:HH:MM", "Listen", "QUSER", "0", "0", "446", "*TCP"),
        ("ddm", "002:HH:MM", "Listen", "QUSER", "0", "0", "447", "*TCP"),
        ("ddm-ssl", "000:HH:MM", "Listen", "QUSER", "0", "0", "448", "*TCP"),
        ("as-svrmap", "000:HH:MM", "Listen", "QUSER", "0", "0", "449", "*TCP"),
        ("rmc", "005:HH:MM", "Listen", "QSYS", "0", "0", "657", "*TCP"),
        ("ftps-co >", "000:HH:MM", "Listen", "QTCP", "0", "0", "990", "*TCP"),
        ("telnet- >", "000:HH:MM", "Listen", "QTCP", "0", "0", "992", "*TCP"),
        ("ssh", "025:HH:MM", "Listen", "QSYS", "0", "0", "2222", "*TCP"),
    ]
    .iter()
    .map(|(port, idle, state, user, out, inb, num, ty)| {
        vec![
            ("CRADR", "*"),
            ("CRPORT", "*"),
            ("CLPORT", *port),
            ("CIDLE", *idle),
            ("CSTATE", *state),
            ("CUSER", *user),
            ("COUT", *out),
            ("CIN", *inb),
            ("CLADR", "*"),
            ("CRPORTN", "*"),
            ("CLPORTN", *num),
            ("CTYPE", *ty),
        ]
    })
    .collect();
    let refs: Vec<&[(&str, &str)]> = rows.iter().map(Vec::as_slice).collect();
    let mut l = list(&refs);
    l.complete = false;
    l.total = None;
    l
}

#[test]
fn netstat_3_cnn() {
    let mut d = MapDialog::new();
    d.set_list("CNNS", conns());
    let s = render(&netstat(), "CNNSTSD", &d, &opts()).unwrap();
    compare(&golden("netstat-3-cnn"), &s);
}

#[test]
fn netstat_3_cnn_f11() {
    let mut d = MapDialog::new();
    d.set_list("CNNS", conns());
    d.set("VIEW", "2");
    let s = render(&netstat(), "CNNSTSD", &d, &view(1)).unwrap();
    compare(&golden("netstat-3-cnn-f11"), &s);
}

#[test]
fn netstat_3_cnn_f14() {
    let mut d = MapDialog::new();
    d.set_list("CNNS", conns());
    d.set("VIEW", "3");
    let s = render(&netstat(), "CNNSTSD", &d, &view(2)).unwrap();
    compare(&golden("netstat-3-cnn-f14"), &s);
}

fn cnn_detail(d: &mut MapDialog, listen: bool) {
    let (host, adr, port) = if listen { ("", "*", "*") } else { ("LOOPBACK", "N  .N.N.N", "N") };
    data(
        d,
        &[
            ("DRHOST", host),
            ("DRADR", adr),
            ("DRPORT", port),
            ("DLHOST", host),
            ("DLADR", adr),
            ("DLPORT", "telnet"),
            ("DUSER", "QTCP"),
            ("DSTATE", if listen { "Listen" } else { "Established" }),
            ("DOPEN", "Passive"),
            ("DIDLE", "N  :N :N .N"),
            ("DLAST", "MM/DD/YY  HH:MM:SS"),
            ("DRTT", "N,N"),
            ("DRTTV", "N,N"),
        ],
    );
    for k in [
        "DBOUT", "DOBUF", "DUSNDNXT", "DSNDNXT", "DSNDUNA", "DOPUSH", "DOURG", "DOWIN", "DBIN", "DIBUF", "DRCVNXT", "DURCVNXT",
        "DIPUSH", "DIURG", "DIWIN", "DRETR", "DCRETR", "DSWMAX", "DSWCUR", "DSWUPD", "DSWACK", "DCWND", "DSSTH", "DMSS",
        "DPREC", "DISS", "DIRS",
    ] {
        d.set(k, "N");
    }
}

#[test]
fn netstat_3_cnn_5() {
    let mut d = MapDialog::new();
    cnn_detail(&mut d, false);
    for (page, name) in [(0, "netstat-3-cnn-5"), (1, "netstat-3-cnn-5-2"), (2, "netstat-3-cnn-5-3")] {
        let s = render(&netstat(), "DSPCNN", &d, &Options { page, ..opts() }).unwrap();
        compare(&golden(name), &s);
    }
}

#[test]
fn netstat_3_listen_5() {
    let mut d = MapDialog::new();
    cnn_detail(&mut d, true);
    for (page, name) in [(0, "netstat-3-listen-5"), (1, "netstat-3-listen-5-2"), (2, "netstat-3-listen-5-3")] {
        let s = render(&netstat(), "DSPCNN", &d, &Options { page, ..opts() }).unwrap();
        compare(&golden(name), &s);
    }
}

#[test]
fn netstat_3_listen_8() {
    let mut d = MapDialog::new();
    data(
        &mut d,
        &[("JCTYPE", "*TCP"), ("JLADR", "*"), ("JLPORT", "23"), ("JRADR", "*"), ("JRPORT", "*")],
    );
    d.set_list(
        "JOBS",
        list(&[&[("JNAME", "QTVTELNET"), ("JUSER", "QTCP"), ("JNBR", "NNNNNN"), ("JTYPE", "*BCH"), ("JCUR", "QTCP")]]),
    );
    let s = render(&netstat(), "DSPJOBS", &d, &opts()).unwrap();
    compare(&golden("netstat-3-listen-8"), &s);
}

#[test]
fn netstat_4_ifc6() {
    let mut d = MapDialog::new();
    d.set_list(
        "IFC6S",
        list(&[&[("XADR", "::1"), ("XPFX", "128"), ("XLIND", "*LOOPBACK"), ("XSTS", "Active"), ("XVLAN", "*NONE")]]),
    );
    let s = render(&netstat(), "IFC6STSD", &d, &opts()).unwrap();
    compare(&golden("netstat-4-ifc6"), &s);
}

#[test]
fn netstat_5_rte6() {
    let mut d = MapDialog::new();
    d.set_list(
        "RTE6S",
        list(&[&[("YDEST", "::1"), ("YMASK", "128"), ("YHOP", "*DIRECT"), ("YSTS", "*ACTIVE")]]),
    );
    let s = render(&netstat(), "RTE6INF", &d, &opts()).unwrap();
    compare(&golden("netstat-5-rte6"), &s);
}

#[test]
fn netstat_6_cnn6() {
    let rows: Vec<Vec<(&str, &str)>> = [
        ("ftp-c >", "000:HH:MM"),
        ("telnet", "000:HH:MM"),
        ("ntp", "000:HH:MM"),
        ("drda", "000:HH:MM"),
        ("ddm", "002:HH:MM"),
        ("ddm-ssl", "000:HH:MM"),
        ("as-sv >", "000:HH:MM"),
        ("ftps- >", "000:HH:MM"),
        ("telne >", "000:HH:MM"),
        ("ssh", "025:HH:MM"),
        ("3077", "003:HH:MM"),
        ("4000", "000:HH:MM"),
        ("5000", "000:HH:MM"),
    ]
    .iter()
    .map(|(p, idle)| {
        vec![
            ("ZRADR", "*"),
            ("ZRPORT", "*"),
            ("ZLPORT", *p),
            ("ZIDLE", *idle),
            ("ZSTATE", if *p == "ntp" { "*UDP" } else { "Listen" }),
        ]
    })
    .collect();
    let refs: Vec<&[(&str, &str)]> = rows.iter().map(Vec::as_slice).collect();
    let mut l = list(&refs);
    l.complete = false;
    l.total = None;
    let mut d = MapDialog::new();
    d.set_list("CNN6S", l);
    let s = render(&netstat(), "CNN6STSD", &d, &opts()).unwrap();
    compare(&golden("netstat-6-cnn6"), &s);
}

#[test]
fn netstat_10_status() {
    let mut d = MapDialog::new();
    data(
        &mut d,
        &[
            ("S4", "Active"),
            ("S6", "Active"),
            ("SLMT", "*NO"),
            ("SACT", "1128555"),
            ("SSTR", "MM/DD/YY  HH:MM:SS"),
            ("SEND", "MM/DD/YY  HH:MM:SS"),
            ("SWSJ", "QSTRTCP"),
            ("SWSU", "QPGMR"),
            ("SWSN", "090583"),
        ],
    );
    let s = render(&netstat(), "DSPSTS", &d, &opts()).unwrap();
    compare(&golden("netstat-10-status"), &s);
}

/// Every panel of the group renders with empty data.
#[test]
fn every_panel_renders() {
    let g = netstat();
    for p in &g.panels {
        render(&g, &p.name, &MapDialog::new(), &opts()).unwrap_or_else(|e| panic!("{}: {e}", p.name));
    }
}
