# Release captures (V1R1M0, items D3 and D4)

What the oracles said for the commands, function usage and exit points
added for V1R1M0. Taken 2026-11-06.

## Command definitions (`cmdd71/`)

`QCDRCMDD` (format `CMDD0100`, the command's XML) on the primary
(V7R1M0, as `QSECOFR`) for `SAVSAVFDTA`, `WRKJRNRCV`, `RCVJRNE`,
`CMPJRNIMG`, `CHGSECA`, `DSPSECA`, `WRKFCNUSG`, `CHGFCNUSG`, `DSPFCNUSG`,
`QTCP/TELNET` and `STRPASTHR`, written by
`tools/conformance/jtopen/src/CmdDef.java`:

```
QS_HS_HOST=... QS_HS_USER=... QS_HS_PASS=... QS_HS_SKIP_SIGNON=1 \
  java -cp CLASSES:jt400.jar CmdDef tools/conformance/oracle/rel/cmdd71 QSYS/SAVSAVFDTA ...
```

The shipped definitions in `command/sys-cpps/cmds/` follow these:
keywords, types, lengths, defaults, special and single values, ranges,
dependencies and their message IDs. Two kinds of parameter are left
out and refused by the analyzer (`CPD0043`): `TYPE(*HEX)` (`RCVJRNE`
`OBJFID` and `OBJJID`, which QS/400's definition language lacks) and
nothing else.

## Facts

| Fact | System | How | Result |
|---|---|---|---|
| `CHGAUDCTL` | primary V7R1M0, pub400 V7R5M0 | typed at Command Entry; `ibm-cmd-list.txt` | not a command on either (`CPD0030`); not added |
| `STRPASTHR RMTLOCNAME(QSNOSUCH)` | primary | Command Entry (`golden/verified/oracle71/d3/strpasthr`) | `CPF8933` (route to the location not found) |
| `TELNET RMTSYS(QSNOSUCH.INVALID)` | primary | Command Entry (`telnet-nohost`) | `TCP101A` (F1 on the message, `msgid-telnet-nohost`) |
| `STRPASTHR`, `TELNET` in batch | primary | `QCMDEXC` over JDBC | `CPF0006` (both interactive only, `ExecBatch="NO"`) |
| Function IDs | primary | `WRKFCNUSG` and `DSPFCNUSG` of each (`golden/verified/oracle71/d3fcn/`) | the 37 administrable functions, below; no host-server IDs (`QIBM_DB_ZDA`, `QIBM_DB_DDMDRDA` come at 7.2) |
| Function IDs, 7.5 | pub400 | `QSYS2.FUNCTION_INFO` | adds `QIBM_DB_ZDA`, `QIBM_DB_DDMDRDA`, `QIBM_DB_SECADM`, `QIBM_LIST_ALL_OBJS*`, `QIBM_NAV_*`, ... (not shipped by QS/400 V1R1M0, a V7R1M0-level system) |
| Exit points | primary | `WRKREGINF EXITPNT(QIBM_QSY*)`, `(QIBM_QWC*)`, `(QIBM_QWT*)`, option 5 on `QIBM_QSY_VLD_PASSWRD` | formats below |
| Exit point attributes | pub400 | `QSYS2.EXIT_POINT_INFO` | maximums, preprocessing programs (`exit-points-75.txt`) |
| `QPWDVLDPGM` | primary | `DSPSYSVAL` | `*NONE`; values name, `*REGFAC`, `*NONE` |
| `DSPSECA` | primary | display (`dspseca`) | user ID 109, group ID 102 next numbers; level 40; password level 0; three `*YES/*NO` attributes |

### Exit points on the primary (V7R1M0)

| Point | Formats |
|---|---|
| `QIBM_QSY_CHG_PROFILE` | `CHGP0100` (after), `CHGP0200` (before) |
| `QIBM_QSY_CHK_PASSWRD` | `CHKP0100` (`CHKP0200` is 7.5 only) |
| `QIBM_QSY_CRT_PROFILE` | `CRTP0100` |
| `QIBM_QSY_DLT_PROFILE` | `DLTP0100` (after), `DLTP0200` (before) |
| `QIBM_QSY_RST_PROFILE` | `RSTP0100` |
| `QIBM_QSY_VLD_PASSWRD` | `VLDP0100` (`VLDP0200` is 7.5 only), no maximum, preprocessing `QSYS/QSYPREAD` and `QSYPRERM` |
| `QIBM_QWC_PRERESTRICT` | `PRER0100` |
| `QIBM_QWC_PWRDWNSYS` | `PWRD0100`, `PWRD0200` |
| `QIBM_QWT_JOBNOTIFY` | `NTFY0100` |
| `QIBM_QWT_PREATTNPGMS` | `ATTN0100` |

### Function IDs on the primary (V7R1M0), in `WRKFCNUSG` order

Product `QIBM_BASE_OPERATING_SYSTEM` unless named; default authority and
`*ALLOBJ` use as `DSPFCNUSG` showed them.

| ID | Group | Default | *ALLOBJ |
|---|---|---|---|
| `QIBM_DIRSRV_ADMIN` | `*NONE` | `*DENIED` | `*NOTUSED` |
| `QIBM_ACCESS_ALLOBJ_JOBLOG` | `QIBM_ALLOBJ` | `*DENIED` | `*USED` |
| `QIBM_ALLOBJ_TRACE_ANY_USER` | `QIBM_ALLOBJ` | `*DENIED` | `*USED` |
| `QIBM_WATCH_ANY_JOB` | `QIBM_ALLOBJ` | `*DENIED` | `*NOTUSED` |
| `QIBM_DB_SQLADM` | `QIBM_DB` | `*DENIED` | `*NOTUSED` |
| `QIBM_DB_SYSMON` | `QIBM_DB` | `*DENIED` | `*NOTUSED` |
| `QIBM_QCST_SERVICE_CLUSTADMIN` | `QIBM_QCST_SERVICE_CLUSTMGMT` | `*DENIED` | `*USED` |
| `QIBM_QCST_SERVICE_CLUSTOPER` | `QIBM_QCST_SERVICE_CLUSTMGMT` | `*DENIED` | `*USED` |
| `QIBM_QYAS_SERVICE_DISKMGMT` | `QIBM_SERVICE` | `*DENIED` | `*NOTUSED` |
| `QIBM_SERVICE_DISK_WATCHER` | `QIBM_SERVICE` | `*DENIED` | `*NOTUSED` |
| `QIBM_SERVICE_DUMP` | `QIBM_SERVICE` | `*DENIED` | `*NOTUSED` |
| `QIBM_SERVICE_JOB_WATCHER` | `QIBM_SERVICE` | `*DENIED` | `*USED` |
| `QIBM_SERVICE_THREAD` | `QIBM_SERVICE` | `*DENIED` | `*NOTUSED` |
| `QIBM_SERVICE_TRACE` | `QIBM_SERVICE` | `*DENIED` | `*NOTUSED` |
| `QIBM_SERVICE_WATCH` | `QIBM_SERVICE` | `*DENIED` | `*NOTUSED` |
| `QIBM_QINAV_WEB_CONFIGURE` (product `QIBM_QINAV_NAVIGATOR_WEB`) | `*NONE` | `*DENIED` | `*USED` |
| `QIBM_QINAV_WEB_FUNCTIONS` (same) | `*NONE` | `*DENIED` | `*USED` |
| `QIBM_QINAV_WEB_INTERFACE` (same) | `*NONE` | `*DENIED` | `*USED` |
| `QIBM_QSY_SYSTEM_CERT_STORE` (product `QIBM_QSY_DIGITAL_CERT_MGR`) | `*NONE` | `*DENIED` | `*USED` |
| `QIBM_QTMF_CLIENT_REQ_0` (product `QIBM_QTM_TCPIP`) | `QIBM_QTMF_FTP_CLIENT` | `*ALLOWED` | `*NOTUSED` |
| `QIBM_QTMF_CLIENT_REQ_10`, `_3`, `_6`, `_7`, `_9` | `QIBM_QTMF_FTP_CLIENT_REQ` | `*ALLOWED` | `*NOTUSED` |
| `QIBM_QTMF_SERVER_REQ_0` | `QIBM_QTMF_FTP_SERVER` | `*ALLOWED` | `*NOTUSED` |
| `QIBM_QTMF_SERVER_REQ_1`, `_2` to `_9` | `QIBM_QTMF_FTP_SERVER_REQ` | `*ALLOWED` | `*NOTUSED` |
| `QIBM_QTMF_SERVER_REQ_10` | `QIBM_QTMF_FTP_SERVER_REQ` | `*DENIED` | `*NOTUSED` |
| `QIBM_QYPS_MGTCTRL_SUPER_USER` (product `QIBM_QYPS_MGTCTRL`) | `*NONE` | `*DENIED` | `*NOTUSED` |

The names and descriptions IBM registers are not reproduced; QS/400's
registry carries its own wording.

## Probe scripts

`tools/conformance/probes/rel/d3-prompts71.tn` (prompts),
`d3-panels71.tn` (displays and messages), `d3-fcnusg71.tn` (every
function), run with

```
tn5250-driver run --target oracle --golden tools/conformance/golden/verified/oracle71/d3 \
  --update --release V7R1M0 --timeout 30000 tools/conformance/probes/rel/d3-panels71.tn
```
