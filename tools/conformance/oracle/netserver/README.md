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
| `zlsl/olst-FORMAT-QUAL.txt`, `zlsl/lsti-FORMAT-QUAL.txt` | `probes/NsList.java`: `QZLSOLST` and `QZLSLSTI` (user space in `QTEMP`) on the primary, every list format with qualifier `*ALL` and blanks, `ZLSL9999`, share `QIBM` and a missing share | the list information, the receivers and user spaces hex-dumped (runs of equal lines as `*`) and decoded; errors as `ERRC0100` IDs and replacement data (no text) |
| `jobd-primary.txt` | `probes/NsJobd.java`: `QWDRJOBD JOBD0100` | `QSYS/QZLSPJ` and `QSYS/QZLSSERVER` attributes |
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

## The list APIs (`zlsl/`)

- Record sizes on V7R1M0: `ZLSL0100` 1188 (164 fixed and a 1024-byte
  path area), `ZLSL0101` variable (198 fixed: no encryption or
  authorization list, which 7.4 and 7.5 add; list information record
  length 0), `ZLSL0200` 736, `ZLSL0201` 772 (no encryption or
  authorization list pairs), `ZLSL0300` 192, `ZLSL0400` 96, `ZLSL0600`
  64, `ZLSL0700` 192, `ZLSL0800` 16, `ZLSL0900` 10.
- Paths are in the job's CCSID (37 here), not UTF-16. The path offset
  of `ZLSL0100` from `QZLSOLST` counts from the entry; in `ZLSL0101`
  from `QZLSOLST` it counts from the receiver's start, and in both
  formats from `QZLSLSTI` from the user space's start.
- Flags are the characters `0` and `1`. Share permission 2 is
  read/write; `QIBM` has maximum users -1, `QDIRSRV` `X'7FFFFFFF'`.
  Entries come in share name order.
- Defaults seen: code page 0, idle time-out 6000, oplock time-out 30,
  browse interval 720000, message authentication 0, minimum message
  severity -1, everything else 0 or blank; current and pending equal.
- `QZLSOLST` errors are `CPFB688` (API name, reason, format): reason 1
  for a format that does not exist, 2 when `QZLSLSTI` refused (format
  `ZLSL0400` with a blank qualifier, `ZLSL0900` with `*ALL`).
  `QZLSLSTI` errors are `CPFB68F` (API name, qualified user space,
  reason, format, 0): reason 1 bad format, 7 qualifier not valid
  (`ZLSL0400` blank), 10 qualifier not blank for `ZLSL0900`.
- The user space of `QZLSLSTI` has a generic header of size 576 (level
  `0300`, the API's name repeated at offset 192), the input parameters
  (43 bytes: user space, library, format, qualifier) at 576 and the list
  at 619; there is no header section. For `ZLSL0101` the list size and
  entry size are 0 and the used size covers the entries.
- A share name that does not exist gives an empty list, not an error.
- The job descriptions (`jobd-primary.txt`): both `RTGDTA(QZLSSTART)`,
  `LOG(4 0 *NOLIST)`, `LOGOUTPUT(*PND)`, `JOBQ(QGPL/QBATCH)`; `QZLSPJ`
  `USER(*RQD) JOBMSGQMX(4)`, `QZLSSERVER` `USER(QPGMR) JOBMSGQMX(8)`.

[MS-SMB2]: https://learn.microsoft.com/openspecs/windows_protocols/ms-smb2
[MS-NLMP]: https://learn.microsoft.com/openspecs/windows_protocols/ms-nlmp
[MS-SPNG]: https://learn.microsoft.com/openspecs/windows_protocols/ms-spng
[MS-SRVS]: https://learn.microsoft.com/openspecs/windows_protocols/ms-srvs
