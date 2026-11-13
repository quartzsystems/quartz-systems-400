# NFS facts from the oracles

Captured read-only on 2026-11-12 for the NFS handoff (`docs/NFS.md`).
Nothing was changed on either system: the probe wrote only to `QTEMP`
on the primary and nothing on pub400; no NFS daemon was started, no
export or mount made. The probe is `probes/NfsProbe.java`, the screens
`../../probes/nfs/prompts-nfs.tn` into `../../golden/nfs/`.

## What each oracle can show

| Oracle | NFS | What it settles |
|---|---|---|
| primary (V7R1M0) | installed, not running: 111 and 2049 refuse; no `QNFS*` job; `/etc/RPCBTAB` and `/etc/STATD` left by an earlier start | the commands and their prompts, the menus, `DSPMFSINF`, the message IDs, the objects, the `/etc` files |
| pub400 (V7R5M0) | installed, not running; the account is not authorized to the NFS commands | the 7.5 message IDs (the same `CPxA1xx` set), `QSYSINC` (`QZNFRTVE`, `QZNFNFSO`), that no SQL service covers NFS |

No oracle runs an NFS daemon, and starting one is a configuration
change this project does not make there. The wire behaviour is built
from the RFCs; each open item is `NEEDS-CODY` in `docs/NFS.md` with the
command sequence that settles it.

## Files

| File | Source | What it holds |
|---|---|---|
| `cmdd71/*.xml` | `QCDRCMDD` through JTOpen's `Command.getXML` on the primary | the definitions of `STRNFSSVR`, `ENDNFSSVR`, `EXPORTFS`/`CHGNFSEXP`, `MOUNT`/`ADDMFS`, `UNMOUNT`/`RMVMFS`, `DSPMFSINF`/`STATFS`, `RLSIFSLCK`, `RPCBIND`, `ENDRPCBIND`, `CVTRPCSRC`/`RPCGEN` |
| `messages.txt` | `MessageFile.getMessage` per ID over `CPFA1xx`, `CPDA1xx`, `CPIA1xx`, `CPCA1xx` | the IDs and severities that concern NFS, RPC, mounts and locks. IBM's text is not kept |
| `objects-primary.txt` | `DSPOBJD OBJ(QSYS/*ALL)` filtered | the commands, menus, daemon programs (`QZNF*`), job descriptions and classes, `QNFSANON` |
| `../qsysinc/H/QZNFRTVE.txt`, `QZNFNFSO.txt` | `QSYSINC/H` on pub400 | the two NFS APIs: Retrieve NFS Export Information (`EXPE0100`, `EXPE0200`) and Perform NFS Options (`NFOP0100`-`NFOP0301`) |
| `uids-primary.txt` | `probes/UidProbe.java` (JTOpen `User.getUserID`, `getGroupID`, `getStatus`) on the primary | the UID, GID and status of every shipped profile: `QSECOFR` 0, `QSYS` 101, `QDFTOWN` 102, `QTMPLPD` 103, the others fixed numbers from 4294770688 up (`QNFSANON` 4294770710); no shipped profile has a GID. Also found there: a UID in use is `CPF22CE` (`&1` is `UID` or `GID`, `&2` the number), `QSECOFR`'s UID cannot change (`CPF224C`) |
| `../../golden/nfs/` | `tn5250-driver run --target oracle` | every prompt (plain, F10, a Page Down; `MOUNT` and `UNMOUNT` with `TYPE(*NFS)`, `STRNFSSVR` with `SERVER(*SVR)` and `SERVER(*RPC)`), the menus `CMDNFS`, `CMDMFS`, `CMDRPC`, and `DSPMFSINF` of `/`, `/QSYS.LIB`, `/QOpenSys`, `/home/QSTEST` |

## Findings

- Commands at V7R1M0: `STRNFSSVR`, `ENDNFSSVR`, `EXPORTFS` (= `CHGNFSEXP`),
  `MOUNT` (= `ADDMFS`), `UNMOUNT` (= `RMVMFS`), `DSPMFSINF` (= `STATFS`),
  `RLSIFSLCK`, `RPCBIND`, `ENDRPCBIND`, `CVTRPCSRC` (= `RPCGEN`). There is
  no `CHGNFSA`, `WRKNFSEXP` or `DSPNFSEXP` on either release.
- `STRNFSSVR SERVER(*ALL *RPC *BIO *GSS *RGY *SVR *MNT *NSM *NLM)
  NBRSVR(1-20, 1) NBRBIO(1-20, 1) RTVRPCREG(*NO *YES) STRJOBTIMO(1-3600
  *NOMAX, 30)`. There is no `RETRY` parameter. `NBRSVR` goes with `*SVR`
  or `*ALL` (`CPDA1BB`), `NBRBIO` with `*BIO` or `*ALL` (`CPDA1BC`),
  `RTVRPCREG` with `*RPC` or `*ALL` (`CPDA1C3`).
- `ENDNFSSVR SERVER(the same) ENDJOBTIMO(1-3600 *NOMAX, 30)`.
- `EXPORTFS OPTIONS(200 chars, *DFT) DIR(path) HOSTOPT((host 256, data
  file CCSID *BINARY *ASCII *JOBCCSID 0-65535, path name CCSID *ASCII
  *JOBCCSID, force synchronous write *SYNC *ASYNC), up to 10, *DFT)`.
  One of `OPTIONS` and `DIR` is required (`CPFA1C5` is the definition's
  dependency message).
- `MOUNT TYPE(*NFS *UDFS) MFS MNTOVRDIR OPTIONS(250 chars, *DFT)
  CCSID(data file *ASCII *JOBCCSID *BINARY 1-65533, *BINARY; path name
  *ASCII *JOBCCSID 1-65533, *ASCII) CODEPAGE(the same two, 1-32767)`;
  `CCSID` and `CODEPAGE` exclude each other (`CPDB416`) and are prompted
  for `*NFS` only. `TYPE` has no `*NETWORK` at V7R1M0 and the first three
  parameters are key parameters with a prompt override program.
- `UNMOUNT TYPE(*NFS *UDFS *ALL) MNTOVRDIR(path *ALL) MFS`; the
  dependencies are `CPDA1B2`-`CPDA1B5` and `CPDA1C7`.
- `RLSIFSLCK RMTLOCNAME(256) OBJ(path) NFSSVRLCK(*YES) LCLBRNGLCK(*YES)
  OBJMFS(*NO)`; exactly one of `RMTLOCNAME` and `OBJ` (`CPDA1B6`,
  `CPDA1B7`); `LCLBRNGLCK` and `OBJMFS` not both `*YES` (`CPDA1D8`).
- `DSPMFSINF OBJ OUTPUT(* *PRINT)`.
- Daemon programs in `QSYS`: `QZNFRPCD`, `QZNFBIOD`, `QZNFNFSD`,
  `QZNFMNTD`, `QZNFNSMD`, `QZNFNLMD`, `QZNFRGYD`, `QZNFGSSD`. Job
  descriptions and classes: `QP0LNFSD`, `QP0LRPCD`. The profile
  `QNFSANON` is the anonymous user. No object in `QUSRSYS`.
- No exit point, function ID or SQL service names NFS on V7R5M0
  (`QSYS2.EXIT_POINT_INFO`, `FUNCTION_INFO`, `SYSTABLES`, `SYSROUTINES`);
  there is no `QIBM_QSTRTNFS`. IBM starts NFS from the startup program
  or by hand. A QS/400 view under `QSYS2` is QS/400's own.
- `/etc` on the primary: `netconfig` (CCSID 819; the transports `tcp`,
  `tcp_ord`, `udp`, each visible, family `inet`), `vfs` (CCSID 1208; the
  file system names and numbers: `root` 1, `qopensys` 2, `qdls` 4, `qsys`
  8, `nfs2` 16, `nfs3` 32, `udfs` 64, `qopt` 128, `rfs` 256, `ntc` 1024,
  `nfs4` 8192; the defaults `udfs` and `nfs3`), `RPCBTAB` and `STATD`.
  There is no `/etc/exports` or `/etc/netgroup` until someone makes one,
  and no `rmtab`, `xtab`, `fstab` or `passwd`.
- `/etc/RPCBTAB` (CCSID 37) is 357-byte records: the text `program
  version netid universal-address owner` ended by a new line (`X'15'`)
  and padded with zeros. The 24 records left by the last start: `100000`
  versions 2 and 3 on `::.0.111` (owner 0), `100003` versions 2, 3 and 4
  on `::.8.1` over `tcp` and `udp`, `100005` versions 1 and 3 (`udp`),
  `100021` versions 1 to 4, `100024` version 1, `100020` version 3; all
  but the binder's own with owner 4294967295. So V7R1M0 still registers
  NFS version 2; QS/400 does not serve it.
- `/etc/STATD` is two bytes, `X'F115'`: the status monitor's state
  number as EBCDIC text and a new line.
- The messages: see `messages.txt`. The ones the commands send:
  `CPCA1B0`/`CPCA1B1` mounted and unmounted, `CPCA1BB`/`CPCA1BC` the
  start and end commands done, `CPIA1BB`/`CPIA1BC` one daemon type
  started or ended, `CPDA1BA`/`CPDA1BD` a daemon already running,
  `CPFA1BC`/`CPFA1BE` start or end failed, `CPFA1B9`/`CPFA1BD` the
  timeout passed, `CPCA1B4`-`CPCA1B6` and `CPFA1C9`-`CPFA1CC` the binder,
  `CPCA1C4`, `CPCA1C6`, `CPDA1C4`, `CPDA1C5` exports, `CPFA1B0`-`CPFA1B3`,
  `CPFA1BB`, `CPFA1C3`, `CPFA1C4` the option lists, `CPFA1C6` the form of
  `MFS`, `CPFA1B8` `*IOSYSCFG`, `CPCA1C7`, `CPCA1C9`, `CPFA1B6` lock
  release, `CPFA1CE` a host name without an address.
