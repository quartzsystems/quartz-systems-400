# NetServer facts from the oracles

Captured read-only on 2026-11-09 for the NetServer handoff
(`docs/NETSERVER.md`). Nothing was changed on either system; the probes
wrote only to `QTEMP`. The probe sources are in `probes/`.

## What each oracle can show

| Oracle | NetServer | What it settles |
|---|---|---|
| primary (V7R1M0) | running: `QZLSSERVER` (`QPGMR`, batch), prestart `QZLSFILE` and `QZLSFILET` (`QUSER`, `BJ`), all in `QSERVER`; 445 and 139 answer | job and subsystem shape, the list APIs (`QZLSOLST`, `QZLSLSTI`), the configuration files, message IDs, SMB1 wire behaviour only |
| pub400 (V7R5M0) | prestart `QZLSFILE` only, `QZLSSERVER` not started; 445 and 139 closed from here | the `QSYS2.SERVER_SHARE_INFO` view, the `QSYSINC` headers, the 7.5 object list |

**The primary speaks SMB1 only.** An SMB2 `NEGOTIATE` offering 2.0.2 to
3.1.1 (or 2.0.2 alone, or 3.0/3.0.2) gets the connection closed; an SMB1
`NEGOTIATE` offering `NT LM 0.12`, `SMB 2.002` and `SMB 2.???` gets an SMB1
answer choosing `NT LM 0.12` (no extended security). No oracle shows SMB2
or SMB3 on the wire, so that part of QS/400 is built from [MS-SMB2],
[MS-NLMP], [MS-SPNG] and [MS-SRVS]; each item is `NEEDS-CODY` in
`docs/NETSERVER.md` with the client test that settles it.

## Files

| File | Source | What it holds |
|---|---|---|
| `objects-primary.txt`, `objects-pub400.txt` | `DSPOBJD OBJ(QSYS/QZLS*) OBJTYPE(*ALL) OUTPUT(*OUTFILE)` | NetServer's programs, service programs and job descriptions. The primary has `QZLSSERVER *JOBD` and `QZLSTCP`; pub400 does not |
| `messages.txt` | `MessageFile.getMessage` per ID | the IDs and severities of `QCPFMSG`'s NetServer messages: `CPIB680`-`CPIB692`, `CPFB680`-`CPFB695`, `CPDB680`. IBM's text is not kept; QS/400 writes its own |
| `audit-vp.txt` | `QSYS2/SYSCOLUMNS` on the primary (`probes/NsAudit.java`) | the fields of `QASYVPJ5`, `QASYVPJE` and `QASYPWJ5`: the `VP` entry (`VPTYPE`, `VPNWSD`, `VPSDAT`, `VPSTIM`, `VPCOMP`, `VPPROF`) is what NetServer writes for a password not valid |
| `server-share-info.txt` | `QSYS2.SYSCOLUMNS` | the 20 columns of `QSYS2.SERVER_SHARE_INFO` (no rows visible to the pub400 account) |
| `../qsysinc/H/QZLS*.txt` | `QSYSINC/H` on pub400 | the headers of the 13 APIs and `QZLS` (common definitions), formats `ZLSL0100`-`ZLSL0900`, `ZLSS0100`, `ZLSS0200` |
| `../net/primary/dspsbsd-qserver.txt`, `../net/pub400/prestart-jobs.txt` | Phase 10 captures | routing entry 800 (`QZLSSERVER`, compare value `QZLSSTART`), prestart entries `QZLSFILE` (`QZLSPJ`, 1/1/5, `MAXUSE` 200) and `QZLSFILET` |
| `windows-negotiate.txt` | `smbtest probe 127.0.0.1 445` against this PC's own Windows SMB server (NEGOTIATE only) | each dialect alone with encryption allowed and with no ciphers offered: Windows answers 3.1.1 with an empty `SMB2_ENCRYPTION_CAPABILITIES` (CipherCount 0, which the smb 0.12.1 client sends when its encryption is disabled) with `STATUS_INVALID_PARAMETER`, as the vendored server does; 2.0.2 gets 64 KiB maximum sizes, later dialects 8 MiB |

## Findings

- APIs in `QSYS`: `QZLSADFS`, `QZLSADPS`, `QZLSCHFS`, `QZLSCHPS`, `QZLSCHSG`
  (guest profile), `QZLSCHSI`, `QZLSCHSN`, `QZLSENDS`, `QZLSENSS` (end a
  session), `QZLSLSTI`, `QZLSOLST`, `QZLSRMS`, `QZLSSTRS`. There is no
  `QZLSOPEN`.
- SQL: `QSYS2.SERVER_SHARE_INFO` is the only NetServer view on 7.5. There
  are no `ADD_SERVER_SHARE`, `CHANGE_SERVER_SHARE` or `REMOVE_SERVER_SHARE`
  procedures and no `SERVER_SESSION_INFO`; `QSYS2.USER_INFO` has the column
  `NETSERVER_DISABLED`. `QSYS2.FUNCTION_INFO` has no NetServer function ID
  and `EXIT_POINT_INFO` no `QIBM_QZLS_*` exit point.
- Persistence: two stream files, `/QIBM/UserData/OS400/NetServer/QAZLSCFG`
  (4104 bytes, the server attributes as current and pending pairs:
  server name, domain, text) and `QAZLSSHR` (15360 bytes, the shares:
  name, text, path), CCSID 37 tag, binary records with UTF-16 strings.
  Nothing in `QUSRSYS`. The files travel with `SAV` of `/QIBM/UserData`,
  not with `SAVSECDTA` or `SAVCFG`.
- Shipped shares on the primary: `QIBM` (`/QIBM`) and `QDIRSRV`
  (`/QIBM/ProdData/OS400/DirSrv`, Directory Services, which QS/400 does
  not ship).
- Server name on the primary equals the system name, not `Q` plus the
  system name. Domain is set; text is `OS/400`.
- System values seen: primary `QPWDLVL` 0, `QMAXSIGN` 3; pub400 `QPWDLVL` 3,
  `QMAXSIGN` 5, `QMAXSGNACN` 1, `QSECURITY` 50.

[MS-SMB2]: https://learn.microsoft.com/openspecs/windows_protocols/ms-smb2
[MS-NLMP]: https://learn.microsoft.com/openspecs/windows_protocols/ms-nlmp
[MS-SPNG]: https://learn.microsoft.com/openspecs/windows_protocols/ms-spng
[MS-SRVS]: https://learn.microsoft.com/openspecs/windows_protocols/ms-srvs
