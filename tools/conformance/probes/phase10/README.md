# Phase 10 probe scripts

Keystroke scripts for `tn5250-driver` (step language in
`tools/tn5250-driver/src/script.rs`; Phase 10 added `{env:NAME}`,
`pick "TEXT" VALUE` and `seek "TEXT" N [KEY]`, with `re:` for a regex).
Captured from pub400.com (V7R5M0) on 2026-10-24, one session at a time
with the driver's 500 ms throttle; the primary oracle has no telnet.
Goldens land in `tools/conformance/golden/phase10/<area>/` with their
provenance in each area's `INDEX.toml`:

```
cargo run -p tn5250-driver -- run --target pub400 --auto-signon \
    --golden tools/conformance/golden/phase10/tcpcfg --update --release V7R5M0 \
    tools/conformance/probes/phase10/tcpcfg.tn
```

Read-only rules the scripts follow: options are typed on a menu only
after its "Selection or command" line shows, change displays are left
with F12, Enter is never pressed on a change prompt, and only display
options (5, 7, 8) are typed in lists. Scripts that press Enter have no
`continue`, so a failed step ends the run. The prompt scripts never
press Enter after `CALL QCMD`, so they use `continue` to step over a
refused prompt. Snapshots that repeated the previous one were dropped.

| Script | Area | What it captures |
|---|---|---|
| `tcpcfg.tn` | `tcpcfg` | `CFGTCP` and options 1 (interfaces, F11, option 5 on 127.0.0.1), 2 (routes, F11, option 5 on `*DFTROUTE`), 4 (port restrictions), 10 (host table, option 5), 21 (related tables menu), 22 (point-to-point menu) |
| `tcpapp.tn` | `tcpcfg` | option 21's service, protocol and network tables; `CFGTCPAPP` (two pages) and the SNMP, RouteD, BOOTP, TELNET and SMTP menus |
| `netstat.tn` | `netstat` | `NETSTAT` and options 1 and 2 with F11 and option 5, the connection list with F11 and F14 |
| `netstat-cnn.tn` | `netstat` | options 5 and 8 on the telnet listener, option 5 on one proxied telnet connection, options 4 to 6 (IPv6) and 10 (stack status), the `*NBRCACHE` refusal, the `NETSTAT` prompt |
| `cmn.tn` | `cmn` | `WRKHDWRSC TYPE(*CMN)` with option 7 (on the first resource and on `CMN04`) and 5 (refused on an IOP; on `CMN04`), `WRKLIND` and `DSPLIND` (option 5), `WRKCFGSTS` for `*LIN` (with F11), `*CTL` and `*DEV` |
| `ifs.tn` | `ifs` | `WRKLNK '/'` with options 5, 8, 9 and 12 (refused) on `/`, `WRKLNK '/*'` with 5, 8, 12 (refused) on `/etc`, `WRKLNK` on a probe stream file in the account's home with 5, 8 and 9, `DSPLNK`, `EDTF` (left with F12), `DSPF` |
| `misc.tn` | `misc` | `WRKREGINF` and options 5 and 8 on `QIBM_QTMF_SERVER_REQ`, the `STRQSH` session (`pwd`, `ls /`), `WRKENVVAR` at `*JOB` and `*SYS`, `WRKRDBDIRE` with option 5, `GO TCPADM`, the `GO CFGTCP` refusal, `PING` of 127.0.0.1 with F10 and the job log |
| `prompts-tcp.tn`, `prompts-srv.tn`, `prompts-ifs.tn` | `prompts` | F4 prompts of 108 Phase 10 commands (each with F10 and two Page Downs where they differ; 22 refused, kept as `prompt-NAME-refused`) |

`netstat-cnn.tn` masks every IPv4 address but 127.x and 0.x (other
users' addresses). pub400 fronts telnet with a local proxy, so every
session shows as a 127.0.0.1 connection and this session's own
connection cannot be told apart; option 8 was taken on the listener
(the `QTVTELNET` job) rather than on someone else's session.

Refusals recorded as goldens (2026-10-24): `NETSTAT OPTION(*NBRCACHE)`
is not a valid value on V7R5M0; `GO CFGTCP` is refused (no authority to
the `*MENU`, while the `CFGTCP` command works); WRKLNK option 12 is not
allowed on `/` or `/etc`; WRKHDWRSC option 5 is not valid on an IOP.
CFGTCP on V7R5M0 has no option 5 (remote system information) and adds
23 (IP filter, not captured: a load/unload action). The pub400 profile
sees only option 5 on the CFGTCP lists.

The prompt scripts: 86 prompts came back; 22 commands are refused to
the pub400 profile (`Not authorized to command`) and their refusal
message on Command Entry is kept as `prompt-NAME-refused`: `ADDEXITPGM`,
`RMVEXITPGM`, `CRTUDFS`, `MOUNT`, `UNMOUNT`, `RST`, `SNDSMTPEMM`,
`DSPHDWRSC`, `STRTCP`, `ENDTCP`, `STRTCPIFC`, `ENDTCPIFC`, `STRTCPSVR`,
`ENDTCPSVR`, `STRHOSTSVR`, `ENDHOSTSVR`, `STRCMNTRC`, `ENDCMNTRC`,
`PRTCMNTRC`, `DLTCMNTRC`, `TRCCNN`, `TRCTCPAPP`. A refusal's F12 leaves
Command Entry for MAIN, so the prompts after it in the same script were
taken from MAIN's command line (the same displays). The F10 and Page
Down snapshots equal to the one before them were dropped, so a command
without additional parameters has one golden.

The primary oracle (V7R1M0) has no telnet, and was unreachable through
its host servers on 2026-10-24 and 2026-10-29: no screen or fact of
it is captured here.
