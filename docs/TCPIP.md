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
