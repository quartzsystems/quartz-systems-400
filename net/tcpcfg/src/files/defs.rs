// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The files in `QUSRSYS`. Those marked `ibm` have the record format
//! the oracle's `DSPFFD` shows (`tools/conformance/oracle/net/pub400/
//! config-file-fields.txt`; `tests/layouts.rs` compares them); the
//! others hold what IBM's formats have no room for (IPv6 addresses, the
//! servers' own attributes) and are ours.

use super::layout::{Layout, a, b, g};

/// `QATOCIFC`: IPv4 interfaces.
pub const QATOCIFC: Layout = Layout {
    file: "QATOCIFC",
    format: "QIFC",
    text: "TCP/IP interfaces",
    ibm: true,
    fields: &[
        a("INTERNET", 15, "Internet address"),
        b("BININTADR", 9, "Internet address, binary"),
        a("LIND", 10, "Line description"),
        a("SUBNETMASK", 15, "Subnet mask"),
        b("BINSUBNET", 9, "Subnet mask, binary"),
        a("TOS", 10, "Type of service"),
        b("MTU", 9, "Largest datagram, 0 for the line's"),
        a("AUTOSTART", 1, "Started with TCP/IP"),
        b("IDLVCTTIMO", 4, "Not used"),
        b("MAXSVC", 4, "Not used"),
        a("DDN", 1, "Not used"),
        a("BITSEQ", 4, "Not used"),
        a("IFCTYPE", 1, "Interface kind"),
        b("NETWORKID", 9, "Network, binary"),
        a("LINETYPE", 10, "Line type"),
        a("LOCALIFC", 15, "Associated local interface"),
        b("BINLOCAL", 9, "Associated local interface, binary"),
        a("CNNTYPE", 1, "Not used"),
        a("IFCINFO", 1, "Not used"),
        a("RESERVED1", 20, "Reserved"),
        a("PVCLGLCHLI", 192, "Not used"),
        a("PPPPROF", 10, "Not used"),
        a("PPPRMTIP", 15, "Not used"),
        a("APPDFN", 16, "Not used"),
        a("IFCNAME", 24, "Alias name"),
        a("NETNAME", 24, "Network name"),
        g("TEXT", 50, "Description"),
        a("DHCPFLG", 1, "Address from DHCP"),
        a("DHCPDYNDNS", 10, "DHCP dynamic DNS updates"),
        a("DHCPCLTID", 10, "DHCP client identifier"),
        b("VRTLANID", 9, "VLAN identifier"),
        a("RESERVED2", 490, "QS/400: alias, preferred interfaces"),
    ],
};

/// `QATOCIFC6`: IPv6 interfaces (ours).
pub const QATOCIFC6: Layout = Layout {
    file: "QATOCIFC6",
    format: "QIFC6",
    text: "TCP/IP IPv6 interfaces",
    ibm: false,
    fields: &[
        a("INTERNET", 45, "Internet address or *IP6SAC"),
        a("LIND", 10, "Line description"),
        b("VRTLANID", 9, "VLAN identifier"),
        b("PFXLEN", 9, "Address prefix length"),
        a("AUTOSTART", 1, "Started with TCP/IP"),
        b("MTU", 9, "Largest datagram, 0 for the line's"),
        a("ALIASNAME", 25, "Alias name"),
        a("TOS", 10, "Type of service"),
        b("DADMAXTRN", 9, "Duplicate address detection transmits"),
        a("PVYEXN", 1, "Privacy extensions"),
        a("IP6GENGLBA", 1, "Generate global addresses"),
        a("TEXT", 50, "Description"),
    ],
};

/// `QATOCRTE`: IPv4 routes.
pub const QATOCRTE: Layout = Layout {
    file: "QATOCRTE",
    format: "QROUTE",
    text: "TCP/IP routes",
    ibm: true,
    fields: &[
        a("RTEDEST", 15, "Route destination"),
        b("BINRTEDEST", 9, "Route destination, binary"),
        a("SUBNETMASK", 15, "Subnet mask"),
        b("BINSUBMASK", 9, "Subnet mask, binary"),
        a("TOS", 10, "Type of service"),
        a("NEXTHOP", 15, "Next hop"),
        b("BINNEXTHOP", 9, "Next hop, binary"),
        b("NETWORKID", 9, "Network, binary"),
        b("MTU", 9, "Largest datagram, 0 for the interface's"),
        a("RTETYPE", 1, "Route kind"),
        a("BINDIFC", 15, "Preferred binding interface"),
        b("BINBINDIFC", 9, "Preferred binding interface, binary"),
        b("PRECEDENCE", 4, "Duplicate route priority"),
        b("RIPMETRIC", 4, "Route metric"),
        a("RIPREDIST", 10, "Route redistribution"),
        a("PPPPROF", 10, "Not used"),
        a("PPPCALLER", 24, "Not used"),
        a("PPPCALLIP", 15, "Not used"),
        a("APPDFN", 16, "Not used"),
        a("NETNAME", 24, "Network name"),
        g("TEXT", 50, "Description"),
        a("RESERVED", 10, "Not used"),
        a("RESERVED2", 204, "Reserved"),
    ],
};

/// `QATOCRTE6`: IPv6 routes (ours).
pub const QATOCRTE6: Layout = Layout {
    file: "QATOCRTE6",
    format: "QROUTE6",
    text: "TCP/IP IPv6 routes",
    ibm: false,
    fields: &[
        a("RTEDEST", 45, "Route destination"),
        b("PFXLEN", 9, "Address prefix length"),
        a("TOS", 10, "Type of service"),
        a("NEXTHOP", 45, "Next hop"),
        a("BINDLIND", 10, "Binding line description"),
        b("BINDVLAN", 9, "Binding VLAN identifier"),
        b("MTU", 9, "Largest datagram, 0 for the interface's"),
        b("METRIC", 4, "Route metric"),
        a("REDST", 4, "Route redistribution"),
        b("DUPRTEPTY", 4, "Duplicate route priority"),
        a("TEXT", 50, "Description"),
    ],
};

/// `QATOCHOST`: the host table's IPv4 entries.
pub const QATOCHOST: Layout = Layout {
    file: "QATOCHOST",
    format: "QHOSTS",
    text: "TCP/IP host table",
    ibm: true,
    fields: &[
        a("INTERNET", 15, "Internet address"),
        a("HOSTNME1", 255, "Host name 1"),
        a("HOSTNME2", 255, "Host name 2"),
        a("HOSTNME3", 255, "Host name 3"),
        a("HOSTNME4", 255, "Host name 4"),
        b("IPINTGER", 9, "Internet address, binary"),
        a("TXTDESC", 64, "Description"),
        a("RESERVED", 49, "Reserved"),
    ],
};

/// `QATOCHST6`: the host table's IPv6 entries (ours).
pub const QATOCHST6: Layout = Layout {
    file: "QATOCHST6",
    format: "QHOSTS6",
    text: "TCP/IP host table, IPv6",
    ibm: false,
    fields: &[
        a("INTERNET", 45, "Internet address"),
        a("HOSTNME1", 255, "Host name 1"),
        a("HOSTNME2", 255, "Host name 2"),
        a("HOSTNME3", 255, "Host name 3"),
        a("HOSTNME4", 255, "Host name 4"),
        a("TXTDESC", 64, "Description"),
    ],
};

/// `QATOCTCPIP`: attributes and domain information, one keyword a record.
pub const QATOCTCPIP: Layout = Layout {
    file: "QATOCTCPIP",
    format: "QATTR",
    text: "TCP/IP attributes and domain",
    ibm: true,
    fields: &[a("KEYWORD", 10, "Keyword"), a("DATA", 2038, "Value")],
};

/// `QATOCPORT`: port restrictions.
pub const QATOCPORT: Layout = Layout {
    file: "QATOCPORT",
    format: "QPORT",
    text: "TCP/IP port restrictions",
    ibm: true,
    fields: &[
        b("LOWERPORT", 9, "Lower port"),
        b("UPPERPORT", 9, "Upper port, -1 for one port"),
        a("PROTOCOL", 4, "Protocol"),
        a("USERPROF", 10, "User profile"),
        a("RESERVED", 106, "Reserved"),
    ],
};

/// `QATOCSTART`: servers started with TCP/IP.
pub const QATOCSTART: Layout = Layout {
    file: "QATOCSTART",
    format: "QTOCSTRT",
    text: "TCP/IP servers",
    ibm: true,
    fields: &[
        a("SERVERTYPE", 1, "S standard, U user-defined"),
        a("SERVER", 30, "Server"),
        a("AUTOSTART", 4, "Started with TCP/IP"),
        a("LIBRARY", 10, "Program library"),
        a("PROGRAM", 10, "Program"),
        a("EXTSTRCMD", 64, "QS/400: server name and type"),
        a("EXTENDCMD", 64, "QS/400: description"),
        a("RESERVED", 57, "Reserved"),
    ],
};

/// `QATOCSVRA`: the servers' own attributes (ours).
pub const QATOCSVRA: Layout = Layout {
    file: "QATOCSVRA",
    format: "QSVRA",
    text: "TCP/IP server attributes",
    ibm: false,
    fields: &[
        a("SERVER", 10, "Server"),
        a("KEYWORD", 10, "Keyword"),
        a("DATA", 1000, "Value"),
    ],
};

/// `QATOCPS`: the services table.
pub const QATOCPS: Layout = Layout {
    file: "QATOCPS",
    format: "PRCD",
    text: "TCP/IP services table",
    ibm: true,
    fields: &[
        a("SERVICE", 32, "Service"),
        b("PORT", 9, "Port"),
        a("PROTOCOL", 32, "Protocol"),
        a("ALIAS1", 32, "Alias"),
        a("ALIAS2", 32, "Alias"),
        a("ALIAS3", 32, "Alias"),
        a("ALIAS4", 32, "Alias"),
        a("TEXT", 50, "Description"),
    ],
};

/// `QATOCPP`: the protocol table.
pub const QATOCPP: Layout = Layout {
    file: "QATOCPP",
    format: "PRCD",
    text: "TCP/IP protocol table",
    ibm: true,
    fields: &[
        a("PROTOCOL", 32, "Protocol"),
        b("PCLNBR", 9, "Protocol number"),
        a("ALIAS1", 32, "Alias"),
        a("ALIAS2", 32, "Alias"),
        a("ALIAS3", 32, "Alias"),
        a("ALIAS4", 32, "Alias"),
        a("TEXT", 50, "Description"),
    ],
};

/// `QATOCPN`: the network table.
pub const QATOCPN: Layout = Layout {
    file: "QATOCPN",
    format: "PRCD",
    text: "TCP/IP network table",
    ibm: true,
    fields: &[
        a("NETWORK", 255, "Network"),
        a("ADDRESS", 15, "Internet address"),
        a("ALIAS1", 255, "Alias"),
        a("ALIAS2", 255, "Alias"),
        a("ALIAS3", 255, "Alias"),
        a("ALIAS4", 255, "Alias"),
        a("TEXT", 50, "Description"),
        b("IPINTGER", 9, "Internet address, binary"),
    ],
};

/// `QATOCRSI`: remote system information (kept empty: X.25).
pub const QATOCRSI: Layout = Layout {
    file: "QATOCRSI",
    format: "QRSI",
    text: "TCP/IP remote system information",
    ibm: true,
    fields: &[
        a("INTERNET", 15, "Internet address"),
        b("BININTADR", 9, "Internet address, binary"),
        a("NETADR", 17, "Network address"),
        a("PVCLGLCHLI", 3, "Virtual circuit"),
        a("RVSCRG", 8, "Reverse charges"),
        b("DFTPKTS1", 4, "Transmit packet size"),
        b("DFTPKTS2", 4, "Receive packet size"),
        b("DFTWDWS1", 4, "Transmit window size"),
        b("DFTWDWS2", 4, "Receive window size"),
        a("RESERVED", 73, "Reserved"),
    ],
};

/// Every file, in the order they are created.
pub const ALL: &[&Layout] = &[
    &QATOCIFC,
    &QATOCIFC6,
    &QATOCRTE,
    &QATOCRTE6,
    &QATOCHOST,
    &QATOCHST6,
    &QATOCTCPIP,
    &QATOCPORT,
    &QATOCSTART,
    &QATOCSVRA,
    &QATOCPS,
    &QATOCPP,
    &QATOCPN,
    &QATOCRSI,
];

/// A file's layout by name.
pub fn layout(file: &str) -> Option<&'static Layout> {
    ALL.iter().copied().find(|l| l.file.eq_ignore_ascii_case(file))
}
