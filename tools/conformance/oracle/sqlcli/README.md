# SQL CLI captures

From pub400 (IBM i 7.5), 2026-11-05.

## Programs

The batch `tools/conformance/rpg/b23cli/` (`CLI001` to `CLI005`, with
the kit's own prototypes in `CLIH.rpginc`) is compiled and run there by
`rpg-oracle`; its outcomes are `tools/conformance/rpg/expected/CLI00n.out`
and settle the CLI's behaviour (handles, attributes, buffers,
diagnostics, catalog results, server mode). Probe programs of the same
form, not kept, settled the rest `docs/SYSAPI.md` §13 states.

## Objects

- `QSYS2.PROGRAM_EXPORT_IMPORT_INFO` for `QSYS/QSQCLI`: 121 procedure
  exports (`tools/conformance/oracle/srvpgm/exports.txt`).
- `QSYS2.OBJECT_STATISTICS('QSYS', '*BNDDIR *SRVPGM *PGM', 'QSQ*')`:
  `QSQCLI *SRVPGM` and `QSQSRVR *PGM`; no `QSQCLI *BNDDIR`.
  `QSYS/QUSAPIBD` lists `QSYS/QSQCLI *SRVPGM`
  (`tools/conformance/oracle/srvpgm/qusapibd.txt`).
- The header `QSYSINC/H(SQLCLI)` was read for its numbers only
  (return codes, handle types, SQL and C types, attributes); its text
  is not kept.

## The server-mode jobs

`SELECT * FROM QSYS2.PRESTART_JOB_INFO WHERE SUBSYSTEM_DESCRIPTION =
'QSYSWRK' AND SUBSYSTEM_DESCRIPTION_LIBRARY = 'QSYS'`, the `QSQSRVR`
row:

| Column | Value |
|---|---|
| PRESTART_JOB_PROGRAM_LIBRARY | QSYS |
| PRESTART_JOB_PROGRAM | QSQSRVR |
| USER_PROFILE | QUSER |
| PRESTART_JOB_NAME | QSQSRVR |
| JOB_DESCRIPTION_LIBRARY | QGPL |
| JOB_DESCRIPTION | QDFTSVR |
| START_JOBS | NO |
| INITIAL_JOBS | 5 |
| THRESHOLD | 2 |
| ADDITIONAL_JOBS | 2 |
| MAXIMUM_JOBS | -1 (`*NOMAX`) |
| MAXIMUM_USES | 200 |
| WAIT_FOR_JOB | YES |
| POOL_ID | 1 |
| CLASS_LIBRARY | QSYS |
| CLASS | QSYSCLS20 |

`SELECT JOB_NAME_SHORT, JOB_USER, SUBSYSTEM, JOB_TYPE, FUNCTION_TYPE,
FUNCTION, JOB_STATUS, AUTHORIZATION_NAME FROM
TABLE(QSYS2.ACTIVE_JOB_INFO(SUBSYSTEM_LIST_FILTER => 'QSYSWRK',
JOB_NAME_FILTER => 'QSQSRVR', DETAILED_INFO => 'NONE'))` (other users'
names masked):

| JOB_NAME_SHORT | JOB_USER | SUBSYSTEM | JOB_TYPE | FUNCTION_TYPE | FUNCTION | JOB_STATUS | AUTHORIZATION_NAME |
|---|---|---|---|---|---|---|---|
| QSQSRVR | QUSER | QSYSWRK | PJ | - | - | CNDW | {other} |
| QSQSRVR | QUSER | QSYSWRK | PJ | - | - | PSRW | QUSER |

A job serving a connection waits `CNDW` as the connecting user; an idle
one waits `PSRW` as `QUSER`; neither shows a function. Inside a
server-mode connection `QSYS2.JOB_NAME` names the `QSQSRVR` job (`CLI001`
prints `QUSER/QSQSRVR`) and `QSYS2.SERVER_MODE_JOB_NAME` is set; outside
server mode it is null (`CLI002`).
