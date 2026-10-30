# TCP/IP

QS/400 uses the Linux kernel's TCP/IP stack. Everything that configures
it, reads its state and serves on it is Quartz Rust. This document
starts with the layer next to the kernel; the configuration objects,
commands and servers built on it are described in later sections as
they land.

## Kernel interfaces

The `netlink` crate (`net/netlink`) is the only code that talks to the
kernel about networking. It is a netlink client written from scratch:
a codec (`codec`: `nlmsghdr`, family headers, 4-byte aligned
attributes, nesting) and one client per bus. No netlink crate is used;
the only dependency beyond `thiserror` is `rustix` for the socket calls,
so the crate itself has no `unsafe`. Off Linux the socket cannot be
opened (`Error::Unsupported`); the codec, the `/proc` parsers and the
sysctl mapping work everywhere, and their tests run on the build host.

| Bus | Client | What QS/400 uses it for |
| --- | --- | --- |
| `NETLINK_ROUTE` | `route::Rtnl` | links (list, get, up/down, MTU, MAC), bonds with mode and slaves, 802.1Q VLANs, addresses v4/v6 with flags and lifetimes, routes v4/v6 with gateway, metric, table, MTU and TOS, the neighbor cache |
| `NETLINK_ROUTE`, group `RTNLGRP_LINK` | `route::LinkEvents` | link added/removed, up/down and carrier changes, for line status and recovery |
| `NETLINK_SOCK_DIAG` | `diag::Diag` | TCP and UDP sockets v4/v6 with addresses, state, uid, inode and `tcp_info` counters (NETSTAT); `SOCK_DESTROY` to end a connection |
| `NETLINK_NETFILTER` | `nft::Nft` | the `qs400` nftables table that backs port restrictions |

Two more readers sit beside them: `procnet` parses
`/proc/net/{tcp,tcp6,udp,udp6}` and maps socket inodes to the process
holding them through `/proc/<pid>/fd` (NETSTAT shows the owning job of
a connection), and `sysctl` reads and writes `/proc/sys/net` with the
`CHGTCPA` mappings below.

### Conventions

- Requests carry increasing sequence numbers and wait for their own
  reply; a request socket times out after 10 seconds.
- Errors from the kernel come back as `Error::Kernel(errno)`;
  `is_exists`, `is_not_found` and `is_not_supported` cover the cases
  callers branch on.
- Routes QS/400 adds carry protocol `RTPROT_QS400` (81) so they can be
  told from kernel, boot and router-advertisement routes.
- A route without a gateway but with an interface is installed
  link-scoped. Tables above 255 go in `RTA_TABLE`.
- Bonds are created with `IFLA_INFO_KIND bond`; slaves are taken down,
  enslaved through `IFLA_MASTER` and brought back up. VLANs are
  `IFLA_INFO_KIND vlan` with `IFLA_VLAN_ID` and protocol 802.1Q on a
  parent link. Virtual IP interfaces are `dummy` links.
- `SOCK_DESTROY` needs `CONFIG_INET_DIAG_DESTROY`, which the QS/400
  kernel has. Kernels without it answer `EOPNOTSUPP`
  (`is_not_supported`), and NETSTAT option 4 then reports that the
  connection cannot be ended.

### Port restriction backstop

`ADDTCPPORT` restrictions are enforced in the sockets API: every
`bind()` by a QS/400 job is checked against them. A socket that does not
come through the API (a PASE program, a direct system call) would not
be checked, so nftables drops inbound traffic to restricted ports
unless the receiving socket was bound through the API.

The API marks every socket it binds with `SO_MARK` = `0x5153xxxx`
(`nft::SOCKET_MARK`, "QS" in the high half; the low 16 bits are free).
QS/400 owns one table and chain:

```
table inet qs400 {
    chain input {
        type filter hook input priority filter; policy accept;
        meta l4proto tcp th dport 8000-8100 socket mark & 0xffff0000 != 0x51530000 counter drop comment "qs400 tcp 8000-8100"
    }
}
```

One rule per restriction (`nft::PortRule`: protocol and port range),
identified by its comment so it can be found and removed. Changes go in
one nftables batch, so the table, chain and rule appear together or not
at all.

The match is `socket mark`, not `meta mark`: `SO_MARK` marks the
packets a socket sends, while an inbound packet's own mark is 0. The
`socket` expression looks up the socket the packet will be delivered
to and reads that socket's mark; connections a listener accepts inherit
its mark. When no socket would receive the packet the rule stops
without matching and the kernel answers as it does for any closed port.
This needs `CONFIG_NFT_SOCKET`; the WSL kernel lacks it, which is why
the nftables test proves the drop under QEMU on the QS/400 kernel.

Built in, bonding and dummy each create one device at boot (`bond0`,
`dummy0`; the baked command line cannot set `max_bonds` or
`numdummies` to 0). TCP/IP configuration either uses them or deletes
them at `STRTCP`.

Setting `SO_MARK` needs `CAP_NET_ADMIN`, so the sockets layer sets it
where that capability is held (the storage service or the job before it
drops privileges).

### CHGTCPA and sysctls

`sysctl::CHGTCPA` lists the attributes with a kernel equivalent; the
rest are stored by TCP/IP configuration only.

| CHGTCPA | Path under `/proc/sys/net` | Conversion |
| --- | --- | --- |
| `TCPKEEPALV` | `ipv4/tcp_keepalive_time` | minutes to seconds |
| `TCPURGPTR` | `ipv4/tcp_stdurg` | `*BSD` 0, `*RFC` 1 |
| `TCPRCVBUF` | `ipv4/tcp_rmem` | middle (default) value |
| `TCPSNDBUF` | `ipv4/tcp_wmem` | middle (default) value |
| `TCPR1CNT` | `ipv4/tcp_retries1` | same |
| `TCPR2CNT` | `ipv4/tcp_retries2` | same |
| `TCPCLOTIMO` | `ipv4/tcp_fin_timeout` | same (seconds) |
| `IPDTGFWD` | `ipv4/ip_forward`, `ipv6/conf/all/forwarding` | `*YES` 1 |
| `IPSRCRTG` | `ipv4/conf/all/accept_source_route` | `*YES` 1 |
| `IPRSBTIMO` | `ipv4/ipfrag_time`, `ipv6/ip6frag_time` | same (seconds) |
| `IPTTL` | `ipv4/ip_default_ttl`, `ipv6/conf/default/hop_limit` | same |
| `ARPTIMO` | `ipv4/neigh/default/gc_stale_time` | minutes to seconds |
| `ECN` | `ipv4/tcp_ecn` | `*YES` 1 |

### Kernel options

The fragment (`kernel/config/qs400.config`) has, and `build kernel`
verifies: `INET_DIAG`, `INET_TCP_DIAG`, `INET_UDP_DIAG`,
`INET_DIAG_DESTROY`, `PACKET` (communications trace), `BONDING`,
`VLAN_8021Q`, `DUMMY`, `NETFILTER`, `NF_TABLES`, `NF_TABLES_INET`,
`NF_TABLES_IPV4`, `NF_TABLES_IPV6`, `NFT_SOCKET`, `TLS` with
`CRYPTO_AES`, `CRYPTO_GCM`, `CRYPTO_CHACHA20POLY1305` (and AES-NI),
`NET_SCHED` with only the default FIFO qdisc (the defconfig's
classifiers and actions are turned off), and the NIC drivers `E1000`,
`E1000E`, `IGB`, `IXGBE`, `I40E`, `ICE`, `MLX5_CORE`, `MLX5_CORE_EN`
next to `VIRTIO_NET`. In Kconfig the TCP and UDP diag options are
spelled `INET_TCP_DIAG` and `INET_UDP_DIAG`. These additions grew the
bzImage (no initramfs) from 14,238,720 to 15,897,600 bytes (+1.66 MB, +11.7%).

### Tests

- Codec and parser tests run anywhere: byte-level builder checks, and
  replies captured from the kernel under `net/netlink/tests/fixtures/`
  (a bond, a VLAN, addresses, routes, an `EEXIST` error, `inet_diag`
  with `tcp_info`, `/proc/net/tcp{,6}`, the nftables rule dump).
- `net/netlink/tests/netns.sh` runs the live tests as root in a fresh
  network namespace: dummy links, a bond with two slaves, a VLAN on it,
  addresses, routes, link events with carrier, a listener found through
  `inet_diag` with its inode and pid, and the nftables backstop.
  `netns.sh capture` rewrites the fixtures.
- The same tests ran under QEMU on the QS/400 kernel (KVM, a static
  musl build of the test binaries as `/init`'s children): all pass,
  including the nftables drop and `SOCK_DESTROY`.
- On a kernel without `NFT_SOCKET` or `INET_DIAG_DESTROY` the live tests
  say so and skip those parts; with `QS400_OWN_KERNEL=1` (the QS/400
  kernel under QEMU) they must pass.

## Configuration

The `tcpcfg` crate (`net/tcpcfg`) is TCP/IP configuration without the
store or the kernel: the model and the commands' rules (`model`), the
`QUSRSYS` files and their record codecs (`files`), the run-time state
(`state`), the engine that applies a configuration (`apply`) over a
small trait of kernel operations (`net::NetOps`), the panels' data
(`panel`) and what the sockets layer reads (`resolver`). The storage
service holds the configuration and the state
(`lic/slic/src/stgsvc/tcp`); jobs reach them through the `tcp` request
(tag 690, `stg::tcp`).

### The files

The storage service creates the files in `QUSRSYS` when they are
missing, each with a new system's records: the loopback interface
(`127.0.0.1`, `*LOOPBACK`, mask `255.0.0.0`, MTU 576, alias
`LOCALHOST`, as IBM ships it), the loopback host names (`LOOPBACK`,
`LOCALHOST`, `IPV6-LOOPBACK`, `IPV6-LOCALHOST`), the attributes' and the
domain's defaults, the servers' autostart (`*TELNET` and `*FTP` on), the
services table (the well-known services and the host servers' `as-*`)
and the protocols (`icmp`, `ip`, `tcp`, `udp`). A command rewrites the
files of the table it changes; `STRTCP` reads them all again, so a file
changed by other means takes effect at the next start.

| File | Format | Holds | Layout |
| --- | --- | --- | --- |
| `QATOCIFC` | `QIFC` | IPv4 interfaces and `*IP4DHCP` | IBM's (1024 bytes, 32 fields); the full 25-character alias and `PREFIFC` sit in `RESERVED2` |
| `QATOCIFC6` | `QIFC6` | IPv6 interfaces and `*IP6SAC` | ours |
| `QATOCRTE` | `QROUTE` | IPv4 routes | IBM's (512, 23); `DUPRTEPTY` in `PRECEDENCE`, `METRIC` in `RIPMETRIC` |
| `QATOCRTE6` | `QROUTE6` | IPv6 routes | ours |
| `QATOCHOST` | `QHOSTS` | the host table's IPv4 entries, four names a record, continued in records with the same address | IBM's (1152, 8) |
| `QATOCHST6` | `QHOSTS6` | its IPv6 entries | ours |
| `QATOCTCPIP` | `QATTR` | every `CHGTCPA` keyword and the domain's (`HOSTNAME`, `DMNNAME`, `DMNSCHLIST`, `HOSTSCHPTY`, `INTNETADR`, `PORT`, `PROTOCOL`, `INLDMNSVR`, `DMNSVRRTY`, `DNSSEC`), one keyword a record | IBM's (2048, 2) |
| `QATOCPORT` | `QPORT` | port restrictions; `UPPERPORT` -1 is `*ONLY` | IBM's (128, 5) |
| `QATOCSTART` | `QTOCSTRT` | each server's autostart (`S`) and the user-defined servers (`U`; name and type in `EXTSTRCMD`, text in `EXTENDCMD`) | IBM's (240, 8) |
| `QATOCSVRA` | `QSVRA` | the servers' own attributes, keyword and value | ours |
| `QATOCPS`, `QATOCPP`, `QATOCPN` | `PRCD` | the services, protocol and network tables | IBM's (246, 214, 1344) |
| `QATOCRSI` | `QRSI` | remote system information (X.25; created, kept empty) | IBM's (128) |

`net/tcpcfg/tests/layouts.rs` compares the IBM formats field by field
with the oracle's `DSPFFD` (`tools/conformance/oracle/net/pub400/
config-file-fields.txt`). The IPv6 files exist because IBM's formats
have 15 characters for an address. Graphic `TEXT` fields are UCS-2
(CCSID 13488).

### Commands

The configuration commands run in the storage service
(`lic/slic/src/cpp/tcpcmd.rs`); their definitions are generated from
the oracle's `QCDRCMDD` capture (keywords, types, lengths, defaults,
special values, minimum and maximum; `command/sys-cpps/tests/tcp_defs.rs`
checks them) with our prompt texts. A change needs `*IOSYSCFG`.

| Command | What it does |
| --- | --- |
| `ADDTCPIFC`, `CHGTCPIFC`, `RMVTCPIFC` | interfaces; an active interface's line and mask cannot change, and it cannot be removed |
| `STRTCPIFC`, `ENDTCPIFC` | start (`*AUTOSTART`: every autostart one) and end interfaces; a line that is off is varied on first |
| `ADDTCPRTE`, `CHGTCPRTE`, `RMVTCPRTE` | routes, found by destination, mask (prefix length for IPv6), type of service and next hop |
| `ADDTCPHTE`, `CHGTCPHTE`, `RMVTCPHTE`, `RNMTCPHTE` | the host table; names are kept upper case, up to 65 an address; `CHGTCPHTE HOSTNAME` rebuilds the list position by position (`*SAME` keeps a name, `*BLANK` drops it) |
| `MRGTCPHT` | merges a file: `*OPSYS`/`*AS400` records in the `QHOSTS` format, `*AIX` (`address name...  # text`) or `*NIC` (`HOST : address : names :`) lines; `REPLACE(*YES)` replaces the table, otherwise an address present gains the names it lacks and a new one is added; `FROMOBJ` (a stream file) waits for the IFS service |
| `CPYTCPHT` | copies the IPv4 entries to a file in the `QHOSTS` format |
| `CHGTCPDMN` | the local names, the search list, `*LOCAL`/`*REMOTE`, up to three name servers, port, protocol, retries |
| `CHGTCPA` | every attribute of the definition; below |
| `ADDTCPPORT`, `RMVTCPPORT` | port restrictions (at most 100); a range of a protocol may repeat for more profiles but not overlap partly |
| `ADDTCPSVR`, `CHGTCPSVR`, `RMVTCPSVR` | user-defined servers in `QATOCSTART` |
| `ADDSRVTBLE`, `RMVSRVTBLE`, `ADDPCLTBLE`, `RMVPCLTBLE`, `ADDNETTBLE`, `RMVNETTBLE` | the related tables |
| `STRTCP`, `ENDTCP`, `ENDTCPABN` | in the job: the service applies or ends the configuration; the job starts and ends the servers |
| `CFGTCP`, `CFGTCPAPP`, `CFGTCPSNMP`, `CFGTCPRTD`, `CFGTCPBP`, `CFGTCPTELN`, `CFGTCPSMTP`, `CFGTCPPTP`, `WRKSRVTBLE`, `WRKPCLTBLE`, `WRKNETTBLE` | the menus and lists, below |

Messages are in `QSYS/QTCPMSG` (`runtime/msgf/msgs/qtcpmsg.toml`): the
identifiers the oracle's `QTCP/QTCPMSGF` shows keep its numbers and
substitution layouts (`TCP2640`, `TCP2658`, `TCP8007`, `TCP9509`,
`TCP9515`, `TCP9517`, `TCP9518`, `TCP9640`, `TCP9642`, `TCP9644`,
`TCP9650`, `TCP9880`); the oracle's `QSYS/QTCPMSG` was not captured, so
`TCP1A00`-`TCP1A14` (start, end, interfaces, servers) and
`TCP1D03`-`TCP1D0D` (added, changed, removed, exists, not found, value
not valid) are ours in IBM's families (`docs/NET_OPEN_QUESTIONS.md`).

### Operation

`QTCPCTL` and `QTCPWRK` are system jobs (type `SYS`, no subsystem, as
the oracle runs them on V7R5M0; V7R1M0's `QTCPIP` and `QTCPMONITR` were
not captured). `QTCPWRK` asks the service every five seconds to look at
the lines and carriers under the interfaces. The startup program runs
`STRTCP` (in place of Phase 8's `STRTCPSVR SERVER(*TELNET)`).

`STRTCP` (`apply::start_tcp`): the attributes into their sysctls, the
host name into the kernel's, the loopback link up with `127.0.0.1/8` and
`::1`, every `*LOOPBACK` interface, then with `STRIFC(*YES)` every
autostart interface (IPv6 ones with `STRIP6(*YES)`), the port rules into
nftables and every route whose next hop an active interface reaches.
The service answers with the autostart servers; the job starts each
through its `ServerStarter`. An interface that cannot start is a
diagnostic, not an escape: TCP/IP still starts.

An interface starts on its line's kernel link (`lo` for `*LOOPBACK`,
`dummy0` for `*VIRTUALIP`, created when missing; `link.vid` for a VLAN,
created and removed with the interface), which is brought up and given
the MTU when one is set (the loopback keeps the kernel's MTU; IBM's 576
is a figure). `*IP6SAC` turns on the link's `autoconf` and `accept_ra`;
`*IP4DHCP` fails (no DHCP client yet). Routes go into the main table
with protocol `RTPROT_QS400`, metric `11 - DUPRTEPTY` (IBM's 10 is the
most preferred), the TOS bits of `TOS` (RFC 1349) and the MTU.

`ENDTCP`: the job ends the servers (`ENDSVR(*YES)`), then every
interface ends (addresses TCP/IP added and the links it made are
removed; the loopback addresses stay, the system uses them), routes and
port rules go. `OPTION(*CNTRLD)` and `*IMMED` differ only for the
servers. `ENDTCPABN` is `ENDTCP OPTION(*IMMED)`.

Statuses, as `NETSTAT *IFC` will show them: an interface is `Inactive`,
`Starting`, `Active`, `Ending`, `Failed` (its last error is kept) or
`RCYPND` (its line lost its carrier; it is `Active` again with the
carrier). An interface whose line is varied off ends. A route is
`Inactive`, `Active` or `Failed`. TCP/IP itself is `Inactive`,
`Starting`, `Active` or `Ending`.

`svc.tcp [ifc|rte|port|attr|host]` at the service console lists the
configuration with its status beside what the kernel has (addresses,
routes marked `qs400`, port rules, each mapped sysctl wanted and read).

### CHGTCPA parameters

Every parameter of the captured definition is kept in `QATOCTCPIP`;
those with a kernel equivalent are written at `STRTCP` and at once when
`CHGTCPA` runs while TCP/IP is active.

| Parameter | Kernel (`/proc/sys/net`) | Conversion |
| --- | --- | --- |
| `TCPKEEPALV` | `ipv4/tcp_keepalive_time` | minutes to seconds |
| `TCPURGPTR` | `ipv4/tcp_stdurg` | `*BSD` 0, `*RFC` 1 |
| `TCPRCVBUF` | `ipv4/tcp_rmem` | the middle (default) of the triple |
| `TCPSNDBUF` | `ipv4/tcp_wmem` | the middle (default) of the triple |
| `TCPR1CNT` | `ipv4/tcp_retries1` | same |
| `TCPR2CNT` | `ipv4/tcp_retries2` | same |
| `TCPMINRTM` | stored only | (the kernel's minimum RTO is per route) |
| `TCPCLOTIMO` | `ipv4/tcp_fin_timeout` | seconds |
| `TCPCNNMSG` | stored only | |
| `UDPCKS` | stored only | (the kernel always checksums) |
| `IPPATHMTU` | `ipv4/ip_no_pmtu_disc` | `*YES` 0, `*NO` 1; the interval is stored only |
| `IPDTGFWD` | `ipv4/ip_forward`, `ipv6/conf/all/forwarding` | `*YES` 1 |
| `IPSRCRTG` | `ipv4/conf/all/accept_source_route` | `*YES` 1 |
| `IPRSBTIMO` | `ipv4/ipfrag_time`, `ipv6/ip6frag_time` | seconds |
| `IPTTL` | `ipv4/ip_default_ttl`, `ipv6/conf/default/hop_limit` | same |
| `IPQOSENB`, `IPQOSBCH`, `IPQOSTMR` | stored only | |
| `IPDEADGATE` | stored only | |
| `ARPTIMO` | `ipv4/neigh/default/gc_stale_time` | minutes to seconds |
| `ECN` | `ipv4/tcp_ecn` | `*YES` 1, `*NO` 0 |
| `NFC` | stored only | |
| `LOGPCLERR` | stored only | |
| `DHCPUNQID` | stored only | |
| `IP6ERRLMT`, `IP6ERRRATE` | stored only | |
| `IP6HOPLMT` | `ipv6/conf/default/hop_limit`, `ipv6/conf/all/hop_limit` | same (written after `IPTTL`) |
| `IP6DUPRTLS` | stored only | |
| `IP6TMPAVL` | `ipv6/conf/default/temp_valid_lft` | days to seconds |
| `IP6TMPAPL` | `ipv6/conf/default/temp_prefered_lft` | days to seconds |
| `IP6TMPAXP` | stored only | |

The table under *Kernel interfaces* is netlink's share of this; the
four after it (`IPPATHMTU`, `IP6HOPLMT`, `IP6TMPAVL`, `IP6TMPAPL`) are
`tcpcfg`'s (`model::attr::Attributes::sysctls`).

### Port restrictions

`model::port::PortRestrictions::allows(port, protocol, user)` is the
rule every `bind()` of a QS/400 job is checked against: a port no entry
covers is open; one that entries cover is open to the profiles they name
only. The service answers the list (`TcpOp::PortRules`) for the sockets
layer. The distinct ranges go to the nftables backstop above (one rule a
range and protocol), replaced whole when the table changes.

### What other parts implement or read

| Interface | Implemented by | Now |
| --- | --- | --- |
| `tcpcfg::line::LineState` (a line exists, its kernel link, varied on, vary on) | communications configuration (`*LIND`, `VRYCFG`) | `KernelNames`: a line named like a kernel link (`LIND(ETH0)` is `eth0`), always varied on |
| `tcpcfg::server::ServerStarter` (start, end, active) | the TCP servers framework | the job's `JobStarter`: `*TELNET` through `STRTCPSVR`, the others answer `TCP1A04` |
| `tcpcfg::resolver::HostConfig` (`TcpOp::HostConfig`) | read by the resolver | the host table, `HOSTNAME`, `DMNNAME`, the search list, `*LOCAL`, name servers, port, protocol, `INITQRYINT`, `QRYTIMOUT` (V7R5M0's `CHGTCPDMN` has `DMNSVRRTY` in their place: the interval, and attempts times the interval) |
| `tcpcfg::resolver::port_rules` (`TcpOp::PortRules`) | read by the sockets layer's `PortRules` | the restrictions |
| `tcpcfg::model::tables::ServicesTable` (`TcpOp::Services`) | read by `getservbyname`, `getservbyport` | the services table |
| `ServerAttrs` (`TcpOp::Servers`, `TcpOp::SetServer`) | the servers' `CHGxxxA` commands | autostart and a keyword bag per server |
| `tcpcfg::state::Snapshot` (`TcpOp::Snapshot`) | read by `NETSTAT` | the status of TCP/IP, every interface and route |

### The displays

`CFGTCP` is a menu (`QTOCCFG`): options 1, 2, 4 and 10 show the lists of
interfaces, routes, port restrictions and host table entries
(`QTOCIFC`, `QTOCRTE`, `QTOCPORT`, `QTOCHTE`) with their option 5
displays; 3, 11 and 12 prompt `CHGTCPA`, `MRGTCPHT` and `CHGTCPDMN`; 20
is `CFGTCPAPP` (`QTOCAPP`, with the applications' menus); 21 the
related tables (`QTOCTBL`); 22 `CFGTCPPTP`; 23 says IP filters are not
available. A user without `*IOSYSCFG` sees the lists with `5=Display`
only, as the oracle's profile shows them; with it, `1=Add 2=Change
4=Remove 5=Display 9=Start 10=End` (interfaces) and the like. An
application option whose command this release does not have says so.
`GO TCPADM`'s options run these commands; `GO CFGTCP` (a `*PGM` menu on
IBM) does not exist yet.

The panels follow the pub400 goldens (`ui/uim/tests/tcp_panels.rs`).
Two UIM additions of ours serve them: `:LIST HEADSEP` (a blank row
between the headings and the entries) and `SCROLL=IND` on `:MENU` and
`:DATA` (the `More...`/`Bottom` row kept on every page); and a
non-breaking blank now shows as a blank and is not trimmed from text, so
`Configure BOOTP` keeps its name inline and a heading row can be held
off its field's edge. Two goldens are not matched: the port
restrictions list (the oracle's headings follow no single alignment:
`Opt`, `Lower` and `Protocol` on the middle row, `*ONLY` under `Upper`
on the third, `User` over `Profile` on the first two) and the host
table's second key row (`F22` sits under `F6`, a column R7 does not give
it); the host table's other rows are compared.

## Hardware resources

Every Ethernet port with a device behind it (`/sys/class/net/*/device`)
is a `CMNnn` resource, numbered in PCI address order, then port number
(`dev_port`), then interface name, so names do not move when the
kernel renames interfaces. Ports are named before serial ports, which
share the `CMN` numbering. The PCI function stays `LINnn`; it takes the
type and model of its ports. Each port records its driver, PCI address
and ids, MAC, carrier and speed; the `hw` console word re-reads carrier
and speed before it prints.

Types are four characters in the Quartz range `QNxx`, which IBM does
not use, one per driver family; the model is `001`.

| Type | Driver | Description |
| --- | --- | --- |
| `QN00` | any other | Ethernet port |
| `QN01` | `virtio_net` | Virtio Ethernet port |
| `QN02` | `e1000` | Gigabit Ethernet port |
| `QN03` | `e1000e` | Gigabit Ethernet port |
| `QN04` | `igb` | Gigabit Ethernet port |
| `QN05` | `ixgbe` | 10 Gigabit Ethernet port |
| `QN06` | `i40e` | 40 Gigabit Ethernet port |
| `QN07` | `ice` | 100 Gigabit Ethernet port |
| `QN08` | `mlx5_core` | ConnectX Ethernet port |

## Sockets

ILE programs reach the kernel's sockets through IBM's socket API with
IBM's numbers, structures and `errno` values. Four pieces make it:

| Piece | Where | What |
| --- | --- | --- |
| Descriptor table | `runtime/mi/src/fd/` | one numbering per job for sockets and IFS files; `errno` and `h_errno` cells |
| Socket semantics | `net/sockets` | IBM constants and layouts, the Linux translation, port restrictions, the mark, `AF_UNIX` paths, descriptor passing, the resolver |
| Bound procedures | `runtime/sysapi/src/net/` | every export as a C-style procedure: arguments by value, a value returned |
| Service program table | `runtime/ile/src/binder/sysproc.rs` | export name to IBM service program; the binder resolves them as `*SYSTEM` imports and `DSPPGM` names the program |

### Descriptors and errno

A job has one descriptor table (`mi::fd::FdTable`, in `Job::fds`).
A number names an open description: a socket (`Kind::Socket`, the
kernel descriptor in the job process) or an IFS file (`Kind::Ifs`, the
storage service's handle). New numbers are the lowest free one;
`dup`, `dup2` and `F_DUPFD` share the description, so `O_NONBLOCK`
belongs to the description and `FD_CLOEXEC` to the number. Closing the
last number of a description calls its owner's release hook
(`SocketOps` for sockets, `IfsOps` for files, set by the IFS API
layer); `read`, `write`, `readv` and `writev` on a file number go
through `IfsOps` as well. The table holds at most 2,500 numbers
(`EMFILE` past that); `select`'s `fd_set` covers the first 200.

`errno` is a job-owned `int` (`Job::errno`) at an address that never
moves; `__errno()` returns a space pointer to it and RPG reads
`errno` through that pointer. `h_errno` is the same (`__h_errno()`).
Values are IBM's (`ifs::Errno`: 3401 `EACCES`, 3406 `EAGAIN` which is
also `EWOULDBLOCK`, 3425 `ECONNREFUSED`, ...). A Linux `errno` is
translated where the system call returns (`sockets::sys`) and nowhere
else. `strerror` returns Quartz-written text.

### Byte order

Integers in job storage are big-endian (`ABI.md` §1), as on the
machine, so a value a program stores into `sin_port` or `sin_addr` is
already in network order. `htonl`, `htons`, `ntohl` and `ntohs` are
therefore the identity, exactly as IBM's `<netinet/in.h>` defines them;
a byte swap would send `htons(80)` to port 20480. They are exported as
procedures because RPG prototypes them with `EXTPROC`.
`runtime/sysapi/tests/ile.rs` checks the bytes of a `sockaddr_in` an IR
program built with `htons` and `inet_addr`.

### Address structures

The plain call names read and write BSD 4.3 structures (`sa_family` a
2-byte field); the `qso_*98` names, which `_XOPEN_SOURCE` programs bind
to through `#pragma map`, use BSD 4.4 (`sa_len`, then a 1-byte
`sa_family`). Pointers are 16 bytes on 16-byte boundaries and `int`,
`long`, `size_t` and `socklen_t` 4 bytes, which is why `msghdr` and
`addrinfo` are as wide as they are. `hostent`, `servent`, `inet_ntoa`'s
text and `strerror`'s live in storage the job's network state owns, at
addresses that stay valid for the job (each call rewrites the same
buffer, as IBM's do); `getaddrinfo` lists live until `freeaddrinfo`.
Strings are EBCDIC, NUL-terminated. `recvmsg` delivers no ancillary
data; descriptors pass through `givedescriptor`.

### Port restrictions and the mark

Every `bind` of a TCP or UDP socket to a nonzero port asks
`sockets::PortRules` whether the job's user may have it; a refusal is
`EACCES`. TCP/IP configuration implements the trait over its
`ADDTCPPORT` list; `sockets::Restrictions` is IBM's rule over a plain
list (a port no entry covers is free; a covered one is open only to
the profiles of the entries covering it; `*NONE` is nobody).

After a successful bind the socket gets `SO_MARK` = `SOCKET_MARK`, the
value the nftables backstop lets through (see "Port restriction
backstop"). Setting `SO_MARK` needs `CAP_NET_ADMIN`. Jobs have it: slic
starts every job process as root with the full capability set and no
job drops it. A process without it (a future job that drops
privileges, or a test) passes `mark: None` in `NetSetup`; its binds
still work, but traffic to a restricted port it bound would be dropped
by the backstop, so such a process must keep `CAP_NET_ADMIN` as an
ambient capability or have the storage service bind for it. A failed
mark is logged, never a failed bind.

### AF_UNIX

`AF_UNIX` paths are IFS paths. `sockets::IfsPathMap` turns them into
host paths for the kernel (`to_host`, with whether the call binds or
connects) and back for `accept`, `getsockname` and `recvfrom`
(`to_ifs`), and is told after a bind made the socket file (`bound`).
The IFS layer implements it over its resolver, authority checks and
metadata; `sockets::RootedPaths` maps under one host directory.

### Descriptor passing

`givedescriptor(sd, target)` duplicates the kernel descriptor and
queues it for the job whose 16-byte internal identifier is `target`
(what `QUSRJOBI` returns: the job number and name in EBCDIC); the giver
keeps its own number until it closes it. `takedescriptor(source)`
waits for one from `source`, or from any job when `source` is null,
and gives it the lowest free number. The relay is the
`sockets::DescriptorPassing` trait. `sockets::InProcess` relays between
jobs of one process.

Between job processes the storage service runs the descriptor relay
(`sockets::relay`, a thread of the service on the Unix socket
`/run/qs400/fdrelay`): `givedescriptor` connects, sends the target and
its own identifier with the kernel descriptor riding `SCM_RIGHTS`, and
the relay queues it; `takedescriptor` connects and waits for one queued
for it (or for a pool of prestart jobs, which take as `*PROGRAM`). A
job's `net` setup uses `sockets::Relay` when the relay's socket exists
and `sockets::InProcess` otherwise (tests). The host server daemons
hand their connections to their prestart jobs this way
(`docs/HOSTSVR.md` "Jobs").

### Resolver

`sockets::resolver::Resolver` answers `gethostbyname`,
`gethostbyaddr`, `getaddrinfo`, `getnameinfo`, `res_query` and
`res_search`. It reads `HostConfig` at `res_init`: the host table and
the `CHGTCPDMN` attributes (`DomainConfig`: host name, `DMNNAME`,
`DMNSCHLIST`, `HOSTSCHPTY`, up to three servers, `PORT`, `PROTOCOL`,
`INITQRYINT`, `QRYTIMOUT`, retries). `HOSTSCHPTY(*LOCAL)` tries the host
table first, `*REMOTE` the servers first, each falling back to the
other. Host table names match in any case, with or without the local
domain. A name with a dot is tried as given and then in each search
domain; one without is tried in the search domains first.

The stub resolver is written here: the DNS message codec
(`resolver::codec`, compression on decode), one query per server per
round over UDP, TCP when the answer is truncated or `PROTOCOL(*TCP)` /
`RES_USEVC` asks for it. The first round waits `INITQRYINT` per
server, each later round twice as long, all within `QRYTIMOUT`. `A`,
`AAAA`, `CNAME` chains and `PTR` are understood; other records come
back raw through `res_query`. Errors are IBM's `h_errno` values
(`HOST_NOT_FOUND` 5, `NO_DATA` 10, `NO_RECOVERY` 15, `TRY_AGAIN` 20) and,
for `getaddrinfo`, `EAI_*`. Services come from `ServicesTable`
(`getservbyname`, `getservbyport`); `DefaultServices` is the shipped
table. The network path is Unix only.

### Linux only

The socket calls themselves (`sockets::sys` over rustix, socket2 for
`SO_MARK`) exist only on Linux; elsewhere every call is `ENOSYS` and
no descriptor can exist. The resolver's network path needs Unix. The
constants, layouts, the codec, port rules, path maps, the host-table
resolver and the descriptor table work everywhere.

### Tests

- `net/sockets/tests/spec.rs`: the tables below against `consts::ALL`
  and `layout::layouts()`.
- Unit tests: layouts and codecs, the DNS codec, port rules, path maps,
  services, the resolver over the host table; `mi::fd` for the table.
- `net/sockets/tests/linux.rs` (Linux): TCP echo, UDP, `AF_UNIX` under a
  path map, non-blocking sockets and `poll`, options, port refusals,
  descriptor passing between two jobs, `getaddrinfo("localhost")`.
- `net/sockets/tests/dns.rs` (Unix): the resolver against a name server
  on loopback the test runs.
- `runtime/sysapi/tests/ile.rs` (Linux): IR programs doing an HTTP
  client exchange, a refused connect read through `__errno()`, and a
  UDP round trip with pointer arguments on the stack.

### Constants

IBM's values, from `QSYSINC` (`SYS/SOCKET`, `NETINET/IN`,
`NETINET/TCP`, `SYS/POLL`, `SYS/IOCTL`, `H/FCNTL`, `H/NETDB`,
`H/RESOLV`, `ARPA/NAMESER`).

| Name | Value |
| --- | --- |
| `AF_UNSPEC` | 0 |
| `AF_UNIX` | 1 |
| `AF_INET` | 2 |
| `AF_NS` | 6 |
| `AF_INET6` | 24 |
| `AF_UNIX_CCSID` | 98 |
| `AF_TELEPHONY` | 99 |
| `SOCK_STREAM` | 1 |
| `SOCK_DGRAM` | 2 |
| `SOCK_RAW` | 3 |
| `SOCK_SEQPACKET` | 5 |
| `IPPROTO_IP` | 0 |
| `IPPROTO_ICMP` | 1 |
| `IPPROTO_TCP` | 6 |
| `IPPROTO_UDP` | 17 |
| `IPPROTO_IPV6` | 41 |
| `IPPROTO_ICMPV6` | 58 |
| `IPPROTO_RAW` | 255 |
| `SOL_SOCKET` | -1 |
| `SO_BROADCAST` | 5 |
| `SO_ACCEPTECONNABORTED` | 6 |
| `SO_ACCEPTEPERM` | 7 |
| `SO_DEBUG` | 10 |
| `SO_DONTROUTE` | 15 |
| `SO_ERROR` | 20 |
| `SO_KEEPALIVE` | 25 |
| `SO_LINGER` | 30 |
| `SO_OOBINLINE` | 35 |
| `SO_RCVBUF` | 40 |
| `SO_RCVLOWAT` | 45 |
| `SO_RCVTIMEO` | 50 |
| `SO_REUSEADDR` | 55 |
| `SO_SNDBUF` | 60 |
| `SO_SNDLOWAT` | 65 |
| `SO_SNDTIMEO` | 70 |
| `SO_TYPE` | 75 |
| `SO_USELOOPBACK` | 80 |
| `SO_ACCEPTCONN` | 85 |
| `IP_OPTIONS` | 5 |
| `IP_TOS` | 10 |
| `IP_TTL` | 15 |
| `IP_MULTICAST_IF` | 20 |
| `IP_MULTICAST_TTL` | 25 |
| `IP_MULTICAST_LOOP` | 30 |
| `IP_ADD_MEMBERSHIP` | 35 |
| `IP_DROP_MEMBERSHIP` | 40 |
| `IP_DONTFRAG` | 55 |
| `IP_RECVPKTINFO` | 96 |
| `IP_PKTINFO` | 97 |
| `IPV6_V6ONLY` | 100 |
| `IPV6_RECVHOPLIMIT` | 101 |
| `IPV6_UNICAST_HOPS` | 15 |
| `IPV6_MULTICAST_IF` | 20 |
| `IPV6_MULTICAST_HOPS` | 25 |
| `IPV6_MULTICAST_LOOP` | 30 |
| `IPV6_JOIN_GROUP` | 35 |
| `IPV6_LEAVE_GROUP` | 40 |
| `IPV6_DONTFRAG` | 55 |
| `IPV6_CHECKSUM` | 105 |
| `IPV6_HOPLIMIT` | 115 |
| `IPV6_RECVPKTINFO` | 140 |
| `IPV6_PKTINFO` | 145 |
| `IPV6_TCLASS` | 190 |
| `TCP_MAXSEG` | 5 |
| `TCP_NODELAY` | 10 |
| `TCP_KEEPALIVE` | 15 |
| `MSG_DONTROUTE` | 1 |
| `MSG_OOB` | 4 |
| `MSG_PEEK` | 8 |
| `MSG_TRUNC` | 16 |
| `MSG_CTRUNC` | 32 |
| `MSG_WAITALL` | 64 |
| `MSG_EOR` | 128 |
| `IOV_MAX` | 16 |
| `SOMAXCONN` | 512 |
| `SHUT_RD` | 0 |
| `SHUT_WR` | 1 |
| `SHUT_RDWR` | 2 |
| `SCM_RIGHTS` | 1 |
| `INADDR_ANY` | 0 |
| `INADDR_BROADCAST` | 4294967295 |
| `INADDR_LOOPBACK` | 2130706433 |
| `INADDR_NONE` | 4294967295 |
| `INET_ADDRSTRLEN` | 16 |
| `INET6_ADDRSTRLEN` | 46 |
| `IPPORT_RESERVED` | 1024 |
| `POLLIN` | 1 |
| `POLLOUT` | 2 |
| `POLLPRI` | 4 |
| `POLLWRNORM` | 2 |
| `POLLRDNORM` | 16 |
| `POLLRDBAND` | 32 |
| `POLLWRBAND` | 64 |
| `POLLHUP` | 8192 |
| `POLLERR` | 16384 |
| `POLLNVAL` | -32768 |
| `POLLNORM` | 1 |
| `FD_SETSIZE` | 200 |
| `FD_SET_WORDS` | 7 |
| `FIONREAD` | 131073 |
| `FIONBIO` | 131074 |
| `FIOCCSID` | 131085 |
| `FIOASYNC` | 131075 |
| `SIOCATMARK` | 65537 |
| `F_DUPFD` | 0 |
| `F_GETFD` | 1 |
| `F_SETFD` | 2 |
| `F_GETLK` | 3 |
| `F_SETLK` | 4 |
| `F_SETLKW` | 5 |
| `F_GETFL` | 6 |
| `F_SETFL` | 7 |
| `F_GETOWN` | 8 |
| `F_SETOWN` | 9 |
| `FD_CLOEXEC` | 1 |
| `O_RDONLY` | 1 |
| `O_WRONLY` | 2 |
| `O_RDWR` | 4 |
| `O_NONBLOCK` | 128 |
| `O_APPEND` | 256 |
| `HOST_NOT_FOUND` | 5 |
| `NO_DATA` | 10 |
| `NO_ADDRESS` | 10 |
| `NO_RECOVERY` | 15 |
| `TRY_AGAIN` | 20 |
| `NETDB_MAX_HOST_NAME_LENGTH` | 255 |
| `NETDB_MAX_ARRAY_SIZE` | 64 |
| `NETDB_MAX_HOST_ADDR_ARRAY_SIZE` | 100 |
| `NETDB_MAX_ALIASES` | 4 |
| `NETDB_MAX_PROTO_NAME_LENGTH` | 32 |
| `NETDB_MAX_SERV_NAME_LENGTH` | 32 |
| `EAI_AGAIN` | 1 |
| `EAI_BADFLAGS` | 2 |
| `EAI_FAIL` | 3 |
| `EAI_FAMILY` | 4 |
| `EAI_MEMORY` | 5 |
| `EAI_NONAME` | 6 |
| `EAI_SERVICE` | 7 |
| `EAI_SOCKTYPE` | 8 |
| `EAI_SYSTEM` | 9 |
| `EAI_OVERFLOW` | 10 |
| `EAI_BADEXTFLAGS` | 11 |
| `AI_PASSIVE` | 1 |
| `AI_CANONNAME` | 2 |
| `AI_NUMERICHOST` | 4 |
| `AI_NUMERICSERV` | 8 |
| `AI_V4MAPPED` | 16 |
| `AI_ALL` | 32 |
| `AI_ADDRCONFIG` | 64 |
| `AI_EXTFLAGS` | 128 |
| `AI_MASK` | 255 |
| `NI_NOFQDN` | 1 |
| `NI_NUMERICHOST` | 2 |
| `NI_NAMEREQD` | 4 |
| `NI_NUMERICSERV` | 8 |
| `NI_NUMERICSCOPE` | 16 |
| `NI_DGRAM` | 32 |
| `NI_MASK` | 63 |
| `NI_MAXHOST` | 1025 |
| `NI_MAXSERV` | 32 |
| `RES_AAONLY` | 1 |
| `RES_DEFNAMES` | 2 |
| `RES_DNSRCH` | 4 |
| `RES_IGNTC` | 8 |
| `RES_INIT` | 16 |
| `RES_RECURSE` | 32 |
| `RES_STAYOPEN` | 64 |
| `RES_USEVC` | 128 |
| `RES_DEFAULT` | 38 |
| `MAXDNSRCH` | 6 |
| `MAXNS` | 3 |
| `MAXDNAME` | 256 |
| `NS_PACKETSZ` | 512 |
| `PACKETSZ` | 512 |
| `C_IN` | 1 |
| `T_A` | 1 |
| `T_NS` | 2 |
| `T_CNAME` | 5 |
| `T_SOA` | 6 |
| `T_PTR` | 12 |
| `T_MX` | 15 |
| `T_TXT` | 16 |
| `T_AAAA` | 28 |
| `T_SRV` | 33 |
| `T_ANY` | 255 |

### Layouts

Offsets and sizes in bytes; `sockets::layout` has the same tables.
#### `sockaddr (BSD 4.3)` (16 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 2 | `sa_family` |
| 2 | 14 | `sa_data` |

#### `sockaddr (BSD 4.4)` (16 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 1 | `sa_len` |
| 1 | 1 | `sa_family` |
| 2 | 14 | `sa_data` |

#### `sockaddr_in (BSD 4.3)` (16 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 2 | `sin_family` |
| 2 | 2 | `sin_port` |
| 4 | 4 | `sin_addr` |
| 8 | 8 | `sin_zero` |

#### `sockaddr_in (BSD 4.4)` (16 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 1 | `sin_len` |
| 1 | 1 | `sin_family` |
| 2 | 2 | `sin_port` |
| 4 | 4 | `sin_addr` |
| 8 | 8 | `sin_zero` |

#### `sockaddr_in6 (BSD 4.3)` (28 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 2 | `sin6_family` |
| 2 | 2 | `sin6_port` |
| 4 | 4 | `sin6_flowinfo` |
| 8 | 16 | `sin6_addr` |
| 24 | 4 | `sin6_scope_id` |

#### `sockaddr_in6 (BSD 4.4)` (28 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 1 | `sin6_len` |
| 1 | 1 | `sin6_family` |
| 2 | 2 | `sin6_port` |
| 4 | 4 | `sin6_flowinfo` |
| 8 | 16 | `sin6_addr` |
| 24 | 4 | `sin6_scope_id` |

#### `sockaddr_un (BSD 4.3)` (128 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 2 | `sun_family` |
| 2 | 126 | `sun_path` |

#### `sockaddr_un (BSD 4.4)` (128 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 1 | `sun_len` |
| 1 | 1 | `sun_family` |
| 2 | 126 | `sun_path` |

#### `sockaddr_storage` (304 bytes)

Opaque to callers.

#### `iovec` (32 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 16 | `iov_base` |
| 16 | 4 | `iov_len` |

#### `msghdr` (96 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 16 | `msg_name` |
| 16 | 4 | `msg_namelen` |
| 32 | 16 | `msg_iov` |
| 48 | 4 | `msg_iovlen` |
| 64 | 16 | `msg_control` |
| 80 | 4 | `msg_controllen` |
| 84 | 4 | `msg_flags` |

#### `cmsghdr` (12 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 4 | `cmsg_len` |
| 4 | 4 | `cmsg_level` |
| 8 | 4 | `cmsg_type` |

#### `linger` (8 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 4 | `l_onoff` |
| 4 | 4 | `l_linger` |

#### `timeval` (8 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 4 | `tv_sec` |
| 4 | 4 | `tv_usec` |

#### `pollfd` (8 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 4 | `fd` |
| 4 | 2 | `events` |
| 6 | 2 | `revents` |

#### `fd_set` (28 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 28 | `fdes` |

#### `ip_mreq` (8 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 4 | `imr_multiaddr` |
| 4 | 4 | `imr_interface` |

#### `ipv6_mreq` (20 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 16 | `ipv6mr_multiaddr` |
| 16 | 4 | `ipv6mr_interface` |

#### `hostent` (64 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 16 | `h_name` |
| 16 | 16 | `h_aliases` |
| 32 | 4 | `h_addrtype` |
| 36 | 4 | `h_length` |
| 48 | 16 | `h_addr_list` |

#### `hostent_data` (21008 bytes)

Opaque to callers.

#### `servent` (64 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 16 | `s_name` |
| 16 | 16 | `s_aliases` |
| 32 | 4 | `s_port` |
| 48 | 16 | `s_proto` |

#### `addrinfo` (96 bytes)

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 4 | `ai_flags` |
| 4 | 4 | `ai_family` |
| 8 | 4 | `ai_socktype` |
| 12 | 4 | `ai_protocol` |
| 16 | 4 | `ai_addrlen` |
| 32 | 16 | `ai_canonname` |
| 48 | 16 | `ai_addr` |
| 64 | 16 | `ai_next` |
| 80 | 4 | `ai_eflags` |

## Communications objects

`net/cmncfg` holds the pure parts: the `*LIND` payload, the names of a
line's network controller and device, what a line means to the kernel
(`link::plan`), the message identifiers and the status machine
(`status::State`). The storage service owns the objects and the status
(`lic/slic/src/stgsvc/cmn.rs`); the commands' processing programs are
`lic/slic/src/cpp/cmncmd.rs`, the job's displays
`work/qsjob/src/panels/system/config.rs` (`WRKCFGSTS`) and
`work/qsjob/src/panels/system2/cmn.rs` (`WRKLIND`, `DSPLIND`).

### Line descriptions

`CRTLINETH`, `CHGLINETH`, `DSPLIND`, `WRKLIND` and `DLTLIND` take the
parameters of the pub400 definitions
(`tools/conformance/oracle/net/pub400/command-definitions.txt`), in
their order. The capture leaves out parameters IBM documents for
`CRTLINETH` (`EXCHID`, `MAXCTL`, `COSTCNN`, `COSTBYTE`, `SECURITY`,
`PRPDLY`, `USRDFN1` to `USRDFN3`, `AUTOCRTCTL`, `AUTODLTCTL`,
`LINKSPEED`, `THRESHOLD`, `GENTSTFRM`); they are defined among the
captured ones, prompted on request (`PMTRQS`). `AGGPCY` and `AGGRSCL`
show when `RSRCNAME(*AGG)`, `NWS` and `ASSOCPORT` when
`RSRCNAME(*NWSD)`.

QS/400 acts on `RSRCNAME`, `ONLINE`, `VRYWAIT`, `AGGPCY`, `AGGRSCL`,
`ADPTADR`, `MAXFRAME`, `CMNRCYLMT` and `MSGQ`; every other parameter is
validated and stored, and what has no field of its own (`SSAP`,
`GRPADR`, `SRVOPT`, `NETCTL`, `AUT`) is kept as typed.
`EXCHID(*SYSGEN)` becomes `056` and five hexadecimal digits from the
system and line names. `CHGLINETH` refuses (`CPF2633`) to change
`RSRCNAME`, `ADPTADR`, `MAXFRAME`, `AGGRSCL` or `AGGPCY` while the line
is varied on; `DLTLIND` refuses a varied-on line (`CPF26AB`) and
deletes the controller and device automatic configuration made for it.

The payload of a `*LIND` (type `0x001D`, in `QSYS`) is a fixed record,
then the aggregated resource list (a 2-byte count and 10-byte names)
and the stored parameters as `KWD=value;` text in CP037 after a 2-byte
length.

#### `LineDescription` (272 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | Magic | `QSCMLIND` in ASCII |
| 8 | 2 | Version | 1 |
| 10 | 1 | Online | `ONLINE(*YES)` |
| 11 | 1 | AutoCrtCtl | `AUTOCRTCTL(*YES)` |
| 12 | 1 | GenTstFrm | `GENTSTFRM(*YES)` |
| 13 | 1 | Reserved | zero |
| 14 | 2 | VryWait | seconds, 0 `*NOWAIT` |
| 16 | 10 | RsrcName | `CMNnn`, `*AGG`, `*NWSD` |
| 26 | 10 | Bridge | |
| 36 | 10 | AggStd | `*ETHCHL`, `*LNKAGG` |
| 46 | 10 | AggPcy | `*DFT`, `*SRCPORT`, `*DESTPORT`, `*SRCDESTP`, `*RNDRBN` |
| 56 | 10 | NwsName | |
| 66 | 2 | NwsPort | |
| 68 | 10 | AssocPort | |
| 78 | 12 | AdptAdr | `*ADPT` or 12 hexadecimal digits |
| 90 | 10 | EthStd | |
| 100 | 10 | LineSpeed | |
| 110 | 10 | Duplex | |
| 120 | 4 | MaxFrame | 1496 to 8996 |
| 124 | 2 | MaxCtl | |
| 126 | 8 | ExchId | |
| 134 | 10 | Threshold | |
| 144 | 10 | LinkSpeed | |
| 154 | 1 | CostCnn | |
| 155 | 1 | CostByte | |
| 156 | 10 | Security | |
| 166 | 10 | PrpDly | |
| 176 | 1 | UsrDfn1 | |
| 177 | 1 | UsrDfn2 | |
| 178 | 1 | UsrDfn3 | |
| 179 | 1 | Reserved2 | zero |
| 180 | 4 | AutoDltCtl | minutes, `0xFFFFFFFF` for `*NONE` |
| 184 | 2 | RcyCount | `0xFFFF` for `*SYSVAL` |
| 186 | 2 | RcyInterval | minutes |
| 188 | 10 | MsgqName | `*SYSVAL`, `*SYSOPR` or a name |
| 198 | 10 | MsgqLib | |
| 208 | 50 | Text | |
| 258 | 14 | Reserved3 | zero |

### Network controllers and devices

The first vary on of a line makes, by automatic configuration, a
network controller (`*CTLD` type `*NET`, attached to the line) and a
TCP/IP device under it (`*DEVD` class `*NET`, type `*TCPIP`), as IBM
does for TCP/IP over a LAN line. Both are named from the line, as the
pub400 configuration shows (line `LINETH` with controllers `LINETNET`,
`LINETNET00` to `LINETNET02` and devices `LINETTCP`, `LINETTCP00`):
the first five characters of the line name and `NET` (controller) or
`TCP` (device); when that name is taken by any controller, device or
another line's attachment, a two-digit suffix from `00` to `99` is
added. Controller and device are named independently, so `LINETNET02`
can hold `LINETTCP00`. They are marked as made by automatic
configuration (`Auto` in the controller payload, `Virtual` in the
device's) and go with their line. `CPC2629` and `CPC2630` report them.
`AUTOCRTCTL` is stored only: it is about APPC controllers, which this
release has none of.

The virtual workstation controller `QPACTL01` of Phase 8 and its
displays stay as they were; `WRKCFGSTS` and `VRYCFG` show and vary
them next to the lines' objects, their status from their descriptions.

### Status

Statuses live in the storage service, never on disk: every IPL starts
with every line varied off. `cmncfg::State` holds each line with its
controllers and devices; the service keeps it behind a lock shared
with a thread that reads `RTNLGRP_LINK` events (`stgsvc/cmn/kernel.rs`).

| Status | When |
|---|---|
| `VARIED OFF` | after IPL, `VRYCFG STATUS(*OFF)`, a failed vary on |
| `VARY ON PENDING` | while a vary on brings the interface up |
| `VARIED ON` | up, no carrier yet |
| `ACTIVE` | carrier present |
| `RCYPND` | carrier lost within `CMNRCYLMT`: waiting for it to return |
| `RCYCNL` | carrier lost more often than `CMNRCYLMT` allows |
| `FAILED` | carrier lost with `CMNRCYLMT(0 ...)`: no recovery |
| `VARY OFF PENDING` | defined; a vary off is immediate here |

`CMNRCYLMT(count interval)` counts carrier losses within the last
`interval` minutes (0: since the vary on); `*SYSVAL` takes
`QCMNRCYLMT`. A loss within the count is `RCYPND`, and the carrier
coming back makes the line `ACTIVE` again; the loss past the count is
`RCYCNL`, after which only a vary off and on brings it back. A
controller or device shows its line's status while its own switch is
on (`VRYCFG CFGTYPE(*CTL|*DEV)`, or `RANGE(*NET)` of the line), else
`VARIED OFF`.

Messages go to the line's message queue (`MSGQ`; `*SYSVAL` and
`*SYSOPR` are `QSYS/QSYSOPR`, there is no `QCFGMSGQ` yet) from job
`QSYSARB`, with IBM's identifiers and data layouts from the pub400
`QCPFMSG` and Quartz texts:

| Event | Message |
|---|---|
| carrier present (vary on, or back) | `CPF5908`, controller `&1` contacted on line `&2` (informational) |
| carrier lost, recovery started (`RCYPND`) | `CPI5917`, automatic recovery started for controller `&1` |
| carrier lost past the limit (`RCYCNL`) or with none (`FAILED`) | `CPA58EE` inquiry in the 40-variable layout (`&23` line, `&24` controller), replies `C G R`, default `C` |

`RTVCFGSTS` codes are IBM's: 0 varied off, 10 vary off pending, 20
vary on pending, 30 varied on, 50 sign-on display, 60 active, 80
recovery pending, 90 recovery cancelled, 100 failed.

### Vary

`VRYCFG CFGOBJ(...) CFGTYPE(*LIN|*CTL|*DEV) STATUS(*ON|*OFF)` with
`RANGE` and `VRYWAIT` (the captured definition; `*RESET`, `*NWS` and
the media library values are refused with `CPF2640`). A vary on of a
line:

1. `VARY ON PENDING`; the network controller and device made if
   missing.
2. `link::plan`: the port's interface from the `CMNnn` resource
   (`hwrsrc`), or a bond for `RSRCNAME(*AGG)`; MTU `MAXFRAME` + 4 (the
   1496 default is the 1500-byte Ethernet); the address from `ADPTADR`
   unless `*ADPT`.
3. Netlink: for an aggregate the bond `agg<line>` (lower case, cut to
   15 characters) with mode `balance-xor` for `*ETHCHL` (`*RNDRBN`:
   `balance-rr`) or `802.3ad` for `*LNKAGG`, transmit hash layer 2 for
   `*DFT` and layer 3+4 otherwise, its ports enslaved; the MTU and
   address set while the interface is down; the interface up.
4. With `VRYWAIT` (the command's, else the line's) seconds the vary
   waits for the carrier; the carrier is then read under the status
   lock, so an event the thread saw while the line was pending is not
   lost. `ACTIVE` with a carrier, else `VARIED ON`.

A vary off takes the interface down (a line's bond is taken apart and
deleted). `CPC2607`/`CPC2608` (line), `CPC2609`/`CPC2610`
(controller) and `CPC2605`/`CPC2606` (device) complete; a refusal is a
diagnostic (`CPD2645`, `CPD2646`, `CPD26D4`, `CPD2659`, ...) and the
escape `CPF2640`.

802.1Q: no IBM line parameter names a VLAN; the pub400 interface file
`QATOFIPF` has a VLAN identifier (`TFVLANID`). TCP/IP configuration
makes the sub-interface `<parent>.<id>` through
`stgsvc::cmn::kernel::ensure_vlan` when an interface names one.

At service start the lines described `ONLINE(*YES)` are varied on with
`RANGE(*NET)` and no wait. The kernel's boot-time `bond0` (built-in
bonding makes one; the baked command line cannot set `max_bonds=0`) is
deleted then, unless something is enslaved to it; `dummy0` is left
alone and unused.

### Displays and requests

`WRKCFGSTS` (`QUICFG`), `WRKLIND` and `DSPLIND` (`QUILIND`) match the
pub400 captures in `tools/conformance/golden/phase10/cmn`
(`ui/uim/tests/cmn_panels.rs`): objects indented two columns a level,
the job column (name, user, number) or with F11 the type column, two
key sets switched by F24; `DSPLIND` shows each value after its keyword
until F11, an aggregated line on `DSPLINDA` with its policy and ports.
Without a display the service answers the same commands as text.

| `stg` tag | Request |
|---|---|
| 1180 | `cmn_panel(what, args)`: `CFGSTS` (type, description, range, status), `LIND` (generic name), `DSPLIND` (line) |
| 1181 | `cfg_status(name, ty)`: `RTVCFGSTS`'s code and text |
