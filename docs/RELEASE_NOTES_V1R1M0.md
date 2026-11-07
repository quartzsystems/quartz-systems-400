# QS/400 V1R1M0 release notes

V1R1M0 is the first release of QS/400 meant for use outside
development. It is the system Phases 0 to 11 built, with the gaps those
phases carried closed, the on-disk and on-wire formats frozen, and the
operational pieces a release needs added in Phase 12 (`PHASE12.md`).

## Upgrading

- A V1R0M0 system upgrades in place: boot the V1R1M0 media, choose
  Install Licensed Internal Code, option 1 (slip install). User
  libraries, the IFS, journals and spooled files are kept; every shipped
  object is replaced and its signature checked (`INSTALL.md` §4).
- A `SAVLIB` made on V1R0M0 restores on V1R1M0 (`RSTLIB`); the save
  format did not change.
- V1R0M0 media is refused over a V1R1M0 system (no downgrade).
- The formats frozen at V1R1M0 are listed in `tools/fmtcheck/formats.toml`
  and in each format document's "Frozen at V1R1M0" section. A later
  release changes one only with a version bump and a migration.

## What is new in V1R1M0

**Languages and compilers.** ILE RPG `LIKEFILE` on procedure
parameters, `DATA-INTO` and `DATA-GEN` with shipped JSON and XML parsers
and generators (and QS/400's own `QOAR` interface members), Open Access
handlers, `DUMP(A)` into `QPPGMDMP`. RPG III: the QS00RPG licensed
program with `CRTRPGPGM` and `CVTRPGSRC` (`RPG3.md`). CL `%OFFSET` as a
value and a `CHGVAR` target. PCML for `CRTBNDRPG`/`CRTRPGMOD`
`INFOSTMF`.

**Network.** A DHCPv4 client for `*IP4DHCP` interfaces; the install
response file over HTTP, HTTPS and TFTP, and beside a PXE boot file;
telnet automatic sign-on (`INIT0100`, RFC 4777 substitutes, `QRMTSIGN`);
spooled-file data from the print host server; SMTP relay credentials
(`ADDSMTPLE TYPE(*HOSTAUTH)`); the DDM/DRDA server on 446, DDM files and
record-level access to a remote file, server authentication entries;
`*WSCST` objects with IBM's tag language and the shipped objects; QAP
protocol version 7 (draft), the `Manage` service.

**Work management and performance.** `ENDSBS *ALL` ends every job but
the console's and reaches the restricted state; `SAVSYS` needs it.
Collection Services (`STRPFRCOL`, `ENDPFRCOL`, `CFGPFRCOL`,
`CHKPFRCOL`, `CRTPFRDTA`), `*MGTCOL` objects and the `QAPM*` files;
`WRKSYSACT` under QS00PT1; `QPFRADJ`; job accounting (`QACGLVL`,
`ACGCDE`, `CHGACGCDE`, `QACGJRN`).

**Help and national language support.** Help for every panel, display
file, command, parameter and message (`tools/helpaudit` at 100%); F1, F2
and F11 at the prompter; search indexes (`*SCHIDX`, `QHSS1`) and
`DSPHLPDOC`. CCSIDs kept on message files, panel groups and display
files; `INSLNG` and the secondary language libraries (`QSYS2924`
primary, a `QSYS2931` Spanish scaffold); dates, times and decimals in
the job's formats through one path.

**Hardware and virtualization.** More drivers in the kernel (Intel igc,
Broadcom tg3/bnxt, Realtek r8169, vmxnet3, virtio-scsi, Hyper-V, vsock,
TPM, USB UAS); `QSHWQUAL`, a hardware qualification report from the
install media or the system; a guest agent speaking the QEMU guest agent
protocol (Proxmox and libvirt see it), shown in `WRKHDWRSC *CMN`;
`QUALIFIED_HARDWARE.md`.

**Security and service.** `QSSECCHK`, a security check report;
`*PUBLIC *EXCLUDE` kept on the commands and system objects that ship
with it; problem reports sent to the update service
(`SNDSRVRQS ACTION(*REPORT)`).

**Web.** The HTTP Server (QS00DG1) with HTTP Server Administration on
port 2001, and Navigator for QS/400 (QS00NAV) on 2002 and 2005, ship as
licensed programs on the media (`HTTPADMIN.md`, `NAVIGATOR.md`); both were
checked again on the frozen V1R1M0 tree (QEMU steps 64, 70, 86 to 88).

**Tools.** `tools/fmtcheck` (the format freeze), `tools/fuzz` (a
coverage-guided fuzzer in Rust over 18 parsers), `tools/helpaudit`,
`tools/textaudit`, `tools/kcbuild` (the Knowledge Center, on the install
media), `build soak`, `build release`.

## Deliberately not in V1R1M0

- An SMTP server (the SMTP client sends mail).
- BIOS boot: the media and the system boot under UEFI only (the kernel
  is an EFI-stub UKI, `ARCHITECTURE.md` §5).
- Native GPU drivers: the console uses the firmware's framebuffer
  (`CONSOLE.md`).
- A diskless network install: the response file comes over the network,
  the payload from a CD, a USB stick or BMC virtual media.
- Access for Web (QS00AFW, reserved) and the Knowledge Center served over
  HTTP from `/QQS/ProdData/KnowledgeCenter`: after V1R1M0. The
  Knowledge Center is on the install media (`QS400/KC/`).
- BRMS, save while active, parallel saves, optical media libraries,
  remote journaling, IASPs (`PHASE11.md` Deferred).

## Known limits (`BLOCKED(phase12)`)

| Item | Where |
|---|---|
| 3,785 text literals on display paths still to move into message files and panel sources; the count can only go down (`tools/textaudit/baseline.txt`) | developer tools, service listings, job panels |
| Output fields that run past column 80 into the next row (`DSPSRVPGM` signature, `DSPMSGD` list) | `ui/uim` renderer |
| `WRKSYSSTS`, `WRKSHRPOOL`, `WRKDSKSTS` screens: IBM's are display files on a hand grid; ours are UIM panels with the same data | `ui/uim/pnlgrp/QGWC*` |
| Another job's call stack in `WRKJOB` | `work/qsjob/src/console/jobcmd.rs` |
| A `*USA` time field's record buffer holds `hh:mm:ss`; IBM i holds `hh:mm AM` | the database's buffer conversion |
| IFS directories in product save streams (`SAVLICPGM`/`RSTLICPGM`) | `licpgm/lpstore/src/savfile.rs` |
| The call ABI: a pointer to a by-reference parameter, and by-reference against `* value` through a procedure pointer | `ABI.md` §3; a module format bump |
| HTTPAPI over TLS: no ILE binding of `QSOSSLSR` (`gsk_*`); IBM's `QSYSINC` headers are not shipped | `PHASE9.md` |
| DDM files: CHAIN by record number, READP/READE and SETLL by key end with CPF5149; compilers do not fetch a remote record format | `DDM.md` |
| Open Access: the indicator and feedback areas are not passed; `DUMP(A)` has no file sections | `RPG.md` |
| The prompter shows long parameters on one row (IBM wraps them over several) | `command/cmd` prompter |

## Known differences from IBM i

The deviations each phase records stand (`PHASE1.md` to `PHASE12.md`,
the `*_OPEN_QUESTIONS.md` files). The main ones: the product ID of the
operating system is `QS00SS1` (with `5770SS1` as an alias); save files
are their own object type `*SAVF`; the help, message and report text is
Quartz's own (IBM's layouts, not its words); `INSLNG` is QS/400's front
end to `RSTLICPGM RSTOBJ(*LNG)`; the `QSYS2931` Spanish library is a
scaffold with machine-translated first-level text for 90 messages; the
install media's LIC menu has a fourth option, `QSHWQUAL`; Collection
Services messages `CPIB920`-`924` and `CPFB993`-`995` are QS/400's own
identifiers.

All RPG conformance was measured against pub400.com (IBM i V7R5M0): the
primary oracle (IBM i V7R1M0) has no RPG compiler.

## Before shipping (Cody)

The `NEEDS-CODY` list in `PHASE12.md`: sign the release with the real
key (`QS400_SIGNING_KEY`), the ACS checklist, the hypervisor and Dell
qualification runs, physical tape, the oracle commands that need a
configuration change, and the tag.

## Refused in SQL

The list of `SQL.md` §10, imported as it stands.

<!-- sql-refused:begin: imported from docs/SQL.md section 10 by `cargo run -p kcbuild -- notes --update`; do not edit -->
Every form QS/400 refuses or does not provide, as the rows above state it,
by section. A refusal is an error with the SQLCODE given and a message
that says what is not supported; nothing is accepted and ignored.
Generated from the tables above; the rows are the authority.

### 2. Statements

- `CREATE OR REPLACE VIEW`, `CREATE OR REPLACE ALIAS`: supported; refused SQL0084 when other views depend on the view
- `CREATE OR REPLACE TABLE` (`ON REPLACE PRESERVE/DELETE ROWS`): partial: refused SQL0084 when views or indexes depend on the table
- `CREATE OR REPLACE INDEX`: refused SQL0104, as IBM
- `ALTER TABLE` ADD/DROP/ALTER COLUMN (SET DATA TYPE, SET/DROP DEFAULT, SET/DROP NOT NULL, SET GENERATED, RESTART, DROP IDENTITY), ADD/DROP CONSTRAINT: partial: refused SQL0084 (BLOCKED(rel)) when indexes or views depend on the table, and for SET DATA TYPE of a constraint key column
- `ALTER TABLE ... ADD/DROP PARTITION`, materialized query tables: refused SQL0084
- `RENAME TABLE/INDEX` (`TO SYSTEM NAME`, `FOR SYSTEM NAME`): supported; refused SQL0084 for an object other views read
- `EXPLAIN` (a Quartz statement; pub400 answers SQL0104): partial: a query's plan text with `QQRID`, `QQRCOD`, `QVC13` per line; ACS Visual Explain (`QSYS.QQQDBVE`, undocumented) not provided
- `CREATE SEQUENCE` (long and delimited names with generated system names; `START WITH` below `MINVALUE` accepted, as the primary), `ALTER SEQUENCE`, `DROP SEQUENCE`, `NEXT VALUE FOR`, `PREVIOUS VALUE FOR`: supported, IBM's cache blocks matched; `ALTER SEQUENCE AS` refused SQL0084; a failed statement gives back the value it used, where IBM keeps it used
- `GRANT` on sequences: refused SQL0084
- `CREATE PROCEDURE ... EXTERNAL` (RPGLE, CL; `PARAMETER STYLE SQL`, `GENERAL`, `GENERAL WITH NULLS`): supported, also before the program exists (SQL7909); service program procedures up to 64 parameters in the call; `LANGUAGE C/COBOL/PLI/JAVA`, `DB2GENERAL`, `JAVA`, `DB2SQL` refused SQL0084
- `ALLOCATE`/`ASSOCIATE`/`DESCRIBE` in routines, `EXECUTE ... INTO`, cursors and handlers in functions, `AS LOCATOR`, `AUTONOMOUS`: refused SQL0084
- `CALL` from QAP: partial: the first result set only; OUT/INOUT refused SQL0084
- `CALL` and trigger-firing changes from `RUNSQL`, `RUNSQLSTM`: supported from a job's command line and programs; from the service console and QAP still refused SQL0084
- External table functions; `GENERAL` style external functions; `DBINFO`; external functions during an in-place scan or from the service console: refused SQL0084: BLOCKED(rel)
- Array parameters, array arguments in `CALL`: refused SQL0084: BLOCKED(rel)
- Distinct types as SQL PL variables or parameters, sequence types, in `ALTER TABLE`: refused SQL0084: BLOCKED(rel)
- `FOR EACH STATEMENT`, `OLD/NEW TABLE`, `INSTEAD OF`, several events (`INSERT OR ...`), `SECURED`: refused SQL0084 after IBM's own checks (SQL0628, SQL0696, SQL0159, SQL0156): BLOCKED(rel)
- `GRANT`, `REVOKE` on schemas, routines, sequences, variables, types; column privileges: refused SQL0084
- `LOCK TABLE`: refused SQL0084
- `PREPARE`, `EXECUTE`, `EXECUTE IMMEDIATE`, `DESCRIBE`, cursor statements, `WHENEVER`, `INCLUDE`, locator statements given to a dynamic interface (JDBC, STRSQL, RUNSQLSTM, `PREPARE` text): refused as IBM refuses them: SQL0104, SQL0084, SQL0312 for host variables, SQL0199
- `DESCRIBE CURSOR`, `DESCRIBE PROCEDURE`, `ALLOCATE/ASSOCIATE LOCATORS` in programs: refused SQL0084 (requesters describe `RS#n` cursors internally)
- `SET OPTION` (every program option, SQL5066 order): supported; values QS/400 cannot honour refused SQL0084
- LOB file reference variables: declared; any use refused SQL0084: BLOCKED(rel), the job's runtime has no IFS access
- `CONNECT TO [USER ... USING ...]`, `CONNECT RESET`, `SET CONNECTION`, `RELEASE`, `DISCONNECT` (DRDA client, DUW): partial: programs' embedded SQL and the job-side requesters; queries, cursors, immediate statements and `PREPARE`/`EXECUTE` run remotely; remote input values, scrolling `FETCH`, `DESCRIBE`, `GET DIAGNOSTICS` refused SQL0084; SQL0842, 0843, 0752, 0950, 30080, 30082
- `RDBCNNMTH(*RUW)`: refused SQL0084 (accepted and ignored before)
- Three-part names, `CREATE ALIAS` over a remote table: refused SQL0104
- `TRUNCATE` (all options): supported; SQL0100 on an empty table; `RESTART IDENTITY` on an identity table refused SQL0084 (BLOCKED(rel), joins identity state)
- `REFRESH TABLE`: refused SQL0084
- `CREATE TABLE/VIEW/INDEX/ALIAS` in QTEMP: supported; sequences and variables in QTEMP SQL0707; types, routines and triggers there refused SQL0084; a permanent view over a QTEMP table SQL7008

### 3. Queries

- `LATERAL`, `TABLE(fullselect)`, `TABLE(f(args)) AS x`: supported; a RIGHT or FULL lateral join, and a lateral or USING join as a MERGE source, refused SQL0084
- `FETCH FIRST n PERCENT`: refused SQL0104, as pub400
- `NULLS FIRST`, `NULLS LAST`: refused SQL0199, as pub400
- Common table expressions, chained and recursive, `CYCLE`: supported; `CYCLE ... USING` refused SQL0084
- OLAP: `ROW_NUMBER`, `RANK`, `DENSE_RANK`, `NTILE`, `PERCENT_RANK`, `CUME_DIST`, `LAG`/`LEAD` with `RESPECT/IGNORE NULLS`, `FIRST_VALUE`/`LAST_VALUE`/`NTH_VALUE`, `RATIO_TO_REPORT`, aggregates `OVER` with `ROWS` and `RANGE` frames: supported; `NTH_VALUE ... FROM LAST` refused SQL0084
- `LIKE_REGEX`: refused SQL0104, as pub400
- `ARRAY_EXISTS`, `XMLEXISTS`: refused SQL0084
- `SKIP LOCKED DATA` on queries: supported; on other statements refused SQL0084
- `USE AND KEEP ... LOCKS`, `WAIT FOR OUTCOME`, `USE CURRENTLY COMMITTED`: refused SQL0084
- `UPDATE ... FROM`: refused SQL0199, as pub400

### 4. DDL details

- `CHECK`: supported: comparisons, `AND/OR/NOT`, `IS NULL`, `BETWEEN`, `IN`, `LIKE`, arithmetic, concatenation, `UPPER LOWER LENGTH SUBSTR TRIM LTRIM RTRIM ABS MOD COALESCE`; other functions refused SQL0084
- `NOT ENFORCED`, `DEFERRABLE`, `ENABLE/DISABLE QUERY OPTIMIZATION`: refused SQL0084
- Identity columns: `START`, `INCREMENT`, `MINVALUE/MAXVALUE`, `CYCLE`, `CACHE`, `ORDER`, `BIGINT`/`DECIMAL`, `OVERRIDING SYSTEM/USER VALUE`, `IDENTITY_VAL_LOCAL()`: partial: a range past 19 digits refused SQL0084 (BLOCKED(rel)); `ORDER` kept, without effect on one system
- `IMPLICITLY HIDDEN`: refused SQL0084: BLOCKED(rel), queries have no hidden-column flag yet (IBM accepts it)
- `CREATE TABLE ... AS (query) WITH [NO] DATA`, `DEFINITION ONLY`: partial: an expression column is refused SQL0084 until expression types land
- `CREATE TABLE ... LIKE`, `(LIKE t)`: supported; `INCLUDING IDENTITY`, `EXCLUDING COLUMN DEFAULTS`, `USING TYPE DEFAULTS` refused SQL0084
- `WITH [LOCAL \| CASCADED] CHECK OPTION` (SQL0161): partial: on views that stay logical files; refused SQL0084 on others
- `CREATE INDEX ... WHERE` (sparse): partial: conditions select/omit can express; others refused SQL0084
- Expression (derived) index keys: refused SQL0084: BLOCKED(rel), access paths hold no key expressions
- `INCLUDE` on an ordinary index: refused SQL0104, as IBM
- `VOLATILE`/`NOT VOLATILE` [`CARDINALITY`] on CREATE and ALTER TABLE: kept with the table (`*SQLINF` tag 19 `TABLE`); the planner uses a key that serves regardless of statistics and asks none for the table; `QSYS2.SYSTABLESTAT.VOLATILE` and `SYSPARTITIONSTAT.VOLATILE` show `1`/`0` as 7.5 does (7.1 has no such column); a second VOLATILE clause is SQL0104. `IN schema` is ignored with warning SQ20367 (01680) and `IN ASP n` is SQL0104, as on 7.1 and 7.5. `UNIT ANY|SSD` (tables and indexes) and `PAGESIZE 8|16|32|64|128|256|512` (indexes) are no-ops, as IBM documents them for a store with fixed pages and one media tier; any other unit is SQL0104 and any other page size SQL1583 (428DE), as IBM. Kit: `ddl/VOLATIL1` (pub400), `ddl/TBLOPTS` (primary)

### 5. Functions

- `ENCRYPT_AES`, `ENCRYPT_RC2`, `ENCRYPT_TDES`, `DECRYPT_*`, `GETHINT`: refused SQL0084: IBM's encrypted layout is unpublished, so nothing made here could be read there
- `VERIFY_GROUP_FOR_USER`: refused SQL0084
- `XMLAGG`: refused SQL0084

### 7. Data types

- `CURRENT DECFLOAT ROUNDING MODE` and `SET` (7 modes): supported; a mode from a host variable refused SQL0084
- `FOR MIXED DATA`: refused SQL0604
- A LOB as an index or constraint key: refused SQL0350
- Changes through a read-only view: refused SQL0150
- `CLOB ... FOR BIT DATA`: refused SQL0683
- LOB locators, file reference variables, `FREE LOCATOR`: refused: BLOCKED(rel), the engine's locator table exists; the statements come with dynamic and embedded SQL
- `XML` as a column or `CAST` type, `XMLPARSE(CONTENT)`, `XMLQUERY`, `XMLTABLE`, `XMLEXISTS`, `XMLVALIDATE`, `XMLAGG` and the rest: refused SQL0084
- `ROWID`, `DATALINK`: refused SQL0084
- SQL/JSON path filters, item methods, `**`, `@`, variables, `PASSING`: refused SQL0084 (pub400 runs them)
- `JSON_UPDATE`: refused SQL0204, as pub400 (7.5 has none)

### 8. Catalogs

- `QSYS2.SYSCOLAUTH`: present, always empty: BLOCKED(rel), the store keeps no column authorities
- Other `QSYS2` catalogs, `SYSIBM` metadata views: not present: SQL0204

### 9. Services, tools and interfaces

- `HISTORY_LOG_INFO`: partial: `GENERATE_SYSLOG` and `EOF_DELAY` refused SQL0443
- `SPOOLED_FILE_INFO`, `SYSTEM_STATUS` (table function): partial: timestamp filters refused SQL0443; `RESET_STATISTICS` not applied
- `DISPLAY_JOURNAL`: partial: `GENERATE_SYSLOG`, `EOF_DELAY` refused SQL0443; thread, remote and syslog columns null
- `OBJECT_LOCK_INFO`, `MEMORY_POOL_INFO`, `MEMORY_POOL`, `SYSTEM_ACTIVITY_INFO`, `ASP_INFO`, `SYSDISKSTAT`, `HARDWARE_RESOURCE_INFO`, `MEDIA_LIBRARY_INFO`, `COLLECTION_SERVICES_INFO`, `EXIT_POINT_INFO`, `EXIT_PROGRAM_INFO`, `ENVIRONMENT_VARIABLE_INFO`, `GROUP_PTF_INFO`, `SYSTOOLS.GROUP_PTF_DETAILS`, `TCPIP_INFO`, `IFS_OBJECT_PRIVILEGES`, `IFS_OBJECT_LOCK_INFO`: partial: columns QS/400 keeps no value for are null (listed per service in `services/`); `RESET_STATISTICS => 'YES'` refused SQL0443; job-level environment variables not shown
- `DNS_LOOKUP`: partial: the system resolver; `DOMAIN_SERVER` refused SQL0084
- `SYSTOOLS.CHANGE_USER_PROFILE`: partial: preview or run; special-authority actions other than `*SAME` refused SQL0084
- `GENERATE_SQL`, `GENERATE_SQL_OBJECTS`: partial: tables, views, indexes, sequences, aliases with constraints, labels, grants and the drop, replace, unformatted, unqualified and system-name forms, laid out as pub400; other object types and options refused SQL0084; no member written in QTEMP
- `OVERRIDE_QAQQINI`: partial: tuning options kept as no-ops; options that change results or messages refused SQL0084 unless `*DEFAULT`
- `SET_SERVER_SBS_ROUTING`, `SET_PASE_SHELL_INFO` (SSH always starts QSH; no per-user shell is kept), `SYSTOOLS.LPRINTF`, `SYSTOOLS.GENERATE_SPREADSHEET`: refused SQL0084
- `STRSQL`: saved sessions (SQL6102, SQL6335, SQL7971), F13 options 1 to 4 (session attributes reshaping as pub400's, printer file, remove entries, save to a source file), SELECT output to display, printer (`QPQUPRFIL` report) and file, display formats (edited numbers, every date and time format, `*COMMA`), F19/F20 at pub400's column widths, F4 then F12: partial: output to a file supports options 1, 2, 4, 5; OUTMBR other than `*FILE`/`*FIRST` and OUTOPT(3) refused SQL0084; run at a real TN5250 session in QEMU steps 130 and 138
- `CONNECT`, `SET CONNECTION`, `RELEASE`, `DISCONNECT` in STRSQL: supported through the DRDA client; other statements go to the current remote server; a query there refused SQL0084 (BLOCKED(rel): no column descriptions come back)
- `RUNSQLSTM`, `RUNSQL`: partial: options checked (non-default `USRPRF`, `DYNUSRPRF`, `SAAFLAG`, `FLAGSTD` and values QS/400 cannot honour refused SQL0084; `ALWCPYDTA`, `ALWBLK`, `DLYPRP`, `SYSTIME`, `TGTRLS` without effect); a SELECT in RUNSQLSTM or RUNSQL refused SQL0084 (severity 30), as pub400; not yet run end to end
- `CRTSQLRPGI` options: settings it does not apply refused SQL0084 (`RPGPPOPT`, `CONACC`, `DECFLTRND`, `SQLCURRULE`, `DECRESULT`, `SRTSEQ`, `DFTRDBCOL`, `DYNDFTCOL`, `SQLPATH`, `COMPILEOPT`, `DYNUSRPRF`, `SAAFLAG`, `FLAGSTD`), accepted and ignored before
- DRDA server (JDBC, ACS): partial: `EXCSQLSET`, `CALL` with result sets and output values (`SQLDTARD`), bind into packages; LOB locators refused SQL0084
- Other `SYSIBM` views (`_S` forms, `ROUTINES`, `PARAMETERS`, ...): not present: SQL0204
- The other 81 `QSQCLI` exports (`SQLGetInfo`, `SQLMoreResults`, the wide forms, descriptors, locators): not present: a program that imports one fails to bind (CPD5D02)
- `DSPSQLPKG`: not present: no such command on V7R1M0 either (CPD0030)
- `CRTSQLCI`, `CRTSQLCBLI`: not present
- `STRDBMON`, `ENDDBMON` (output file of the primary's 276-column `QQQDBMN` format): partial: record types 3018, 1000, 3000, 3001, 3003, 3010, 3014; programs, host servers, DDM and QAP monitored, STRSQL, RUNSQL and RUNSQLSTM not; `TYPE(*DETAIL)` writes the same records as `*SUMMARY` (no 3019); `FTR*` filters, `FTRSQLCODE`, `JOB` operator `*NE`, `HOSTVAR` other than `*BASIC`, thresholds, `INCSYSSQL(*YES)` refused CPF9898
- `PRTSQLINF` of `*PGM`/`*SRVPGM`: supported for programs `CRTSQLRPGI` makes from now on, laid out as pub400's; of `*SQLPKG` and `*JOB` refused SQL9011 (BLOCKED(rel))
- `ADDPFTRG` (`RPLTRG`, generated names, `ALWREPCHG`, `TRGUPDCND`, `THDSAFE`, `MLTTHDACN`), `RMVPFTRG`, `CHGPFTRG`, `DSPFD TYPE(*TRG)` with `OUTPUT(*OUTFILE)` (`QAFDTRG`): supported; other `DSPFD` outfile types refused CPD0084
- `*READ` triggers on record I/O, logical files (one based-on member) and SQL queries (once per row the scan keeps): supported; in a join the inner table fires for every row its scan keeps where the primary fires on matches only; refused SQL0084 where no program can be called (the service console)
- SQL changes firing external and SQL triggers: supported for statements of programs, the host server, DRDA, QAP, STRSQL, RUNSQL and RUNSQLSTM; refused SQL0084 from the service console
- Triggers from `CPYF` (insert triggers; a failure stops the copy with CPF2972), DFU and the DDM server: supported; `CPYF MBROPT(*UPDADD)` into a file with update triggers refused CPF502B; FTP and the service console still refused
- `ALWREPCHG(*NO)` on after triggers: supported: the program's own change of the row is not refused, as on the primary

### 9b. Performance

- Plan cache (statement text and naming, 512 MiB or 10,000 entries); `QSYS2.DUMP_PLAN_CACHE`, `CLEAR_PLAN_CACHE`, `CHANGE_PLAN_CACHE_SIZE` (`*JOBCTL`): partial: the parsed statement is cached, the access plan chosen again each run; the entry-selection parameters refused SQL0084; `QIBM_DB_SQLADM` not accepted (no function usage registry)
<!-- sql-refused:end -->

## Marker inventory

Every marker left in the tree, generated from it.

<!-- inventory:begin: generated by `cargo run -p kcbuild -- notes --update`; do not edit -->
181 markers in the tree.

| Marker | Where | Reason |
|---|---|---|
| `BLOCKED(rel)` | `database/db/src/odp/mod.rs:742` | a logical path over more than one member |
| `BLOCKED(rel)` | `database/db/src/util/cpyf/mod.rs:538` | an update by *UPDADD does not fire update |
| `BLOCKED(rel)` | `database/sql/src/dbmon/write.rs:100` | records are written with INSERT, which |
| `BLOCKED(rel)` | `database/sql/src/exec/alter.rs:417` | dependent files are not rebuilt", |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/objects2.rs:13` | variables record no |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/objects2.rs:17` | ( |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/admin.rs:58` | adding or removing one special authority needs |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/admin.rs:133` | a statement runs in the storage service, which |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/gensql/format.rs:54` | the standards, obfuscation, temporal, additional |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/gensql/mod.rs:117` | routines, triggers, types, variables, schemas, |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/jobs.rs:246` | no table of ended jobs). |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/procs.rs:151` | PRINT => 'PRINT' writes a spooled file of the |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/qaqqini.rs:186` | these options change results or messages |
| `BLOCKED(rel)` | `database/sql/src/exec/catalog/services/sysconf.rs:291` | the system's resolver answers; asking one server |
| `BLOCKED(rel)` | `database/sql/src/exec/column.rs:93` | an 8-byte counter" |
| `BLOCKED(rel)` | `database/sql/src/exec/column.rs:116` | queries have no implicitly hidden column yet".into(), |
| `BLOCKED(rel)` | `database/sql/src/exec/ddl.rs:544` | ALTER TABLE keeps no COLTYPE entries yet. |
| `BLOCKED(rel)` | `database/sql/src/exec/func/services.rs:89` | the encoding modes pub400 accepts are not |
| `BLOCKED(rel)` | `database/sql/src/exec/grant.rs:63` | routines, sequences, schemas and packages as |
| `BLOCKED(rel)` | `database/sql/src/exec/grant.rs:72` | the store keeps no column authorities. |
| `BLOCKED(rel)` | `database/sql/src/exec/locator.rs:16` | locator host variables (`SQL TYPE IS CLOB_LOCATOR`), |
| `BLOCKED(rel)` | `database/sql/src/exec/mod.rs:1437` | statements run where no trigger program can |
| `BLOCKED(rel)` | `database/sql/src/exec/qtemp.rs:447` | routines, types and triggers in QTEMP need their |
| `BLOCKED(rel)` | `database/sql/src/exec/reads.rs:15` | the primary reads a join's inner table by the join's |
| `BLOCKED(rel)` | `database/sql/src/exec/reads.rs:45` | a context with no job (the service console, the |
| `BLOCKED(rel)` | `database/sql/src/exec/reads.rs:63` | a join or multi-format logical file's rows are |
| `BLOCKED(rel)` | `database/sql/src/exec/rename.rs:35` | the views over it keep its name in their query. |
| `BLOCKED(rel)` | `database/sql/src/exec/routine/call.rs:167` | this interface runs statements in the database |
| `BLOCKED(rel)` | `database/sql/src/exec/routine/create.rs:637` | distinct types need strong typing in the engine. |
| `BLOCKED(rel)` | `database/sql/src/exec/routine/create.rs:788` | an external table function is called for each row |
| `BLOCKED(rel)` | `database/sql/src/exec/routine/extfunc.rs:52` | PARAMETER STYLE GENERAL gives the result as the |
| `BLOCKED(rel)` | `database/sql/src/exec/routine/extfunc.rs:78` | an external table function is called once per row |
| `BLOCKED(rel)` | `database/sql/src/exec/routine/extfunc.rs:173` | a statement run where no program can be called |
| `BLOCKED(rel)` | `database/sql/src/exec/routine/extfunc.rs:181` | a scan holding the store open cannot give it back. |
| `BLOCKED(rel)` | `database/sql/src/exec/schema.rs:26` | IBM's SYSCHKCST, SYSCSTDEP, |
| `BLOCKED(rel)` | `database/sql/src/exec/table.rs:326` | IBM keeps the views and indexes over a replaced |
| `BLOCKED(rel)` | `database/sql/src/exec/trigger/mod.rs:120` | a statement trigger is called once per statement |
| `BLOCKED(rel)` | `database/sql/src/exec/trigger/mod.rs:155` | transition tables of a row trigger. |
| `BLOCKED(rel)` | `database/sql/src/exec/trigger/mod.rs:177` | an insert, update or delete of a view runs its |
| `BLOCKED(rel)` | `database/sql/src/exec/trigger/mod.rs:256` | a column whose storage is not a routine variable's. |
| `BLOCKED(rel)` | `database/sql/src/exec/udt/mod.rs:14` | a variable, routine parameter or result, sequence or |
| `BLOCKED(rel)` | `database/sql/src/exec/udt/typing.rs:387` | a quantified or `IN` subquery's column is not typed |
| `BLOCKED(rel)` | `database/sql/src/exec/view.rs:34` | IBM keeps views over a replaced view when |
| `BLOCKED(rel)` | `database/sql/src/exec/view.rs:127` | a view that is not a logical file is read only, so |
| `BLOCKED(rel)` | `database/sql/src/exec/view.rs:231` | an expression column has no type until the |
| `BLOCKED(rel)` | `database/sql/src/options.rs:110` | the job's connections follow the distributed unit |
| `BLOCKED(rel)` | `database/sql/src/parse/ddl.rs:296` | derived keys need key expressions in the |
| `BLOCKED(rel)` | `database/sql/src/parse/mod.rs:580` | an XML column needs a stored field type (DB_FORMAT |
| `BLOCKED(rel)` | `database/sql/src/parse/mod.rs:594` | a distinct type is a column's or a CAST's |
| `BLOCKED(rel)` | `database/sql/src/parse/trigger.rs:43` | one trigger for several events, with the |
| `BLOCKED(rel)` | `database/sql/src/pl/compile.rs:693` | a CALL passes values, and an array |
| `BLOCKED(rel)` | `database/sql/tests/trigger.rs:266` | in exec::reads); every row its scan keeps here. |
| `BLOCKED(rel)` | `docs/DB_OPEN_QUESTIONS.md:283` | the database |
| `BLOCKED(rel)` | `docs/PHASE12.md:95` | SAVLICPGM writes no IFS records, RSTLICPGM restores none |
| `BLOCKED(rel)` | `docs/PHASE12.md:100` | at jobcmd.rs:153 |
| `BLOCKED(rel)` | `docs/PHASE12.md:121` | the DST options DST.md lists (save/restore storage, disk unit data, remote service, LIC fixes, partitions, ASP moves); their reasons in DST.md name earlier phases and are not re-checked |
| `BLOCKED(rel)` | `docs/PHASE12.md:134` | (b855a775) |
| `NEEDS-CODY` | `docs/PHASE12.md:156` | the primary has no QAUDJRN |
| `BLOCKED(rel)` | `docs/PHASE12.md:165` | `DB_OPEN_QUESTIONS.md` (Utilities) \| what an RPG program sees in a `*USA` time field's buffer \| §1: an RPG program on pub400 \| settled on pub400; the buffer form |
| `NEEDS-CODY` | `docs/PHASE12.md:167` | (a JTOpen probe) |
| `NEEDS-CODY` | `docs/PHASE12.md:169` | needs a test user profile on the primary |
| `NEEDS-CODY` | `docs/PHASE12.md:171` | physical tape acceptance |
| `NEEDS-CODY` | `docs/PHASE12.md:176` | `UI_OPEN_QUESTIONS.md` 4, 6, 8, 12 \| start-up response record; keys that carry data; the printer data stream; SST's audit code \| §1: the primary's telnet \| 4 closed; 6 open (a display-file probe); 8 a |
| `NEEDS-CODY` | `docs/PHASE12.md:242` |  |
| `NEEDS-CODY` | `docs/PHASE12.md:251` | ": Proxmox, ESXi, Workstation, VirtualBox, Hyper-V Gen2 and the Dell checklists; also try booting the ISO as a CD on real firmware \| open |
| `BLOCKED(rel)` | `docs/PHASE3.md:147` | `RCVJRNE`'s |
| `BLOCKED(rel)` | `docs/PHASE9.md:217` | the ABI items below |
| `BLOCKED(rel)` | `docs/PHASE9.md:218` | no ILE binding of `QSOSSLSR` (`gsk_*`) exists; `net/tls` has the semantics only (`docs/CERTS.md`), so `CONFIG_H` is built without `HAVE_SSLAPI` |
| `BLOCKED(rel)` | `docs/PHASE9.md:220` | `SIIOF` and `SIIMF` copy `QSYSINC/QRPGLESRC,FCNTL` and `SYSSTAT`, IBM's headers, which QS/400 does not ship; `JSONPARSE` is a `DATA-INTO` parser (see "Blocked" above) |
| `BLOCKED(rel)` | `docs/PHASE9.md:260` | (a change of the call ABI, `ABI.md` §3, which the V1R1M0 freeze of the module format holds to a version bump): a pointer taken to a by-reference parameter |
| `BLOCKED(rel)` | `docs/PHASE9.md:268` | (the same ABI change): a parameter passed by reference and a pointer |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:7` | its own disk and powered down cleanly. Everything listed as |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:22` | Proxmox VE 8 \| OVMF \| virtio-scsi (default), virtio-blk, SATA \| virtio-net \| `Default` VGA, serial \| QEMU guest agent option |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:23` | VMware ESXi 8 \| EFI \| PVSCSI, LSI Logic SAS, SATA, NVMe \| vmxnet3, e1000e \| SVGA through the GOP \| none (VMware Tools is not offered); vsock |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:24` | VMware Workstation 17 \| EFI \| as ESXi \| vmxnet3, e1000e \| as ESXi \| none; vsock |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:25` | Oracle VirtualBox 7 \| EFI \| AHCI (SATA), virtio-scsi, NVMe \| Intel PRO/1000 (e1000), virtio-net \| VBoxVGA/VMSVGA through the GOP \| none |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:26` | Microsoft Hyper-V, Generation 2 \| UEFI \| SCSI (storvsc) \| synthetic (netvsc) \| Gen2 EFI framebuffer \| none; vsock (hv_sock) |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:27` | Dell PowerEdge with PERC H330 in HBA mode, 4 x 1 TB SSD (reference physical target, `HARDWARE_NOTES.md`) \| UEFI \| MegaRAID (`megaraid_sas`) \| the board's Broadcom or Intel ports \| iDRAC/Matrox VGA thr |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:129` | hypervisor checklists |
| `NEEDS-CODY` | `docs/QUALIFIED_HARDWARE.md:184` | the Dell with the PERC H330 |
| `NEEDS-CODY` | `docs/SQL.md:57` | `sql::objects` |
| `BLOCKED(rel)` | `docs/SQL.md:60` | when indexes or views depend on the table, and for SET DATA TYPE of a constraint key column \| primary \| `integrity/CSTALTER`, `integrity/IDENTITY` |
| `BLOCKED(rel)` | `docs/SQL.md:88` | none \| |
| `BLOCKED(rel)` | `docs/SQL.md:92` | primary \| `sqlpl/ARRAYS` |
| `BLOCKED(rel)` | `docs/SQL.md:97` | primary \| |
| `BLOCKED(rel)` | `docs/SQL.md:99` | primary \| `trigger/TRGDEFS` |
| `BLOCKED(rel)` | `docs/SQL.md:113` | the job's runtime has no IFS access \| pub400 \| `rpg/b11sql/SQL060` |
| `BLOCKED(rel)` | `docs/SQL.md:118` | joins identity state) \| pub400 \| `query/CHANGES` |
| `BLOCKED(rel)` | `docs/SQL.md:169` | `ORDER` kept, without effect on one system \| primary \| `integrity/IDENTITY` |
| `BLOCKED(rel)` | `docs/SQL.md:175` | queries have no hidden-column flag yet (IBM accepts it) \| pub400 \| `integrity/AUDITCOL` |
| `BLOCKED(rel)` | `docs/SQL.md:185` | access paths hold no key expressions \| primary \| `ddl/INDEXEXT` |
| `BLOCKED(rel)` | `docs/SQL.md:253` | the engine's locator table exists; the statements come with dynamic and embedded SQL \| none \| |
| `BLOCKED(rel)` | `docs/SQL.md:279` | the store keeps no column authorities \| pub400 \| `sql::objects` |
| `BLOCKED(rel)` | `docs/SQL.md:327` | no column descriptions come back) \| none \| |
| `BLOCKED(rel)` | `docs/SQL.md:350` | pub400 \| `rpgc::prtsqlinf` |
| `BLOCKED(rel)` | `docs/SQL.md:403` | when indexes or views depend on the table, and for SET DATA TYPE of a constraint key column |
| `BLOCKED(rel)` | `docs/SQL.md:413` | External table functions; `GENERAL` style external functions; `DBINFO`; external functions during an in-place scan or from the service console: refused SQL0084: |
| `BLOCKED(rel)` | `docs/SQL.md:414` | Array parameters, array arguments in `CALL`: refused SQL0084: |
| `BLOCKED(rel)` | `docs/SQL.md:415` | Distinct types as SQL PL variables or parameters, sequence types, in `ALTER TABLE`: refused SQL0084: |
| `BLOCKED(rel)` | `docs/SQL.md:416` | `FOR EACH STATEMENT`, `OLD/NEW TABLE`, `INSTEAD OF`, several events (`INSERT OR ...`), `SECURED`: refused SQL0084 after IBM's own checks (SQL0628, SQL0696, SQL0159, SQL0156): |
| `BLOCKED(rel)` | `docs/SQL.md:422` | the job's runtime has no IFS access |
| `BLOCKED(rel)` | `docs/SQL.md:426` | joins identity state) |
| `BLOCKED(rel)` | `docs/SQL.md:447` | `ORDER` kept, without effect on one system |
| `BLOCKED(rel)` | `docs/SQL.md:448` | queries have no hidden-column flag yet (IBM accepts it) |
| `BLOCKED(rel)` | `docs/SQL.md:453` | access paths hold no key expressions |
| `BLOCKED(rel)` | `docs/SQL.md:470` | the engine's locator table exists; the statements come with dynamic and embedded SQL |
| `BLOCKED(rel)` | `docs/SQL.md:478` | the store keeps no column authorities |
| `BLOCKED(rel)` | `docs/SQL.md:493` | no column descriptions come back) |
| `BLOCKED(rel)` | `docs/SQL.md:502` | `PRTSQLINF` of `*PGM`/`*SRVPGM`: supported for programs `CRTSQLRPGI` makes from now on, laid out as pub400's; of `*SQLPKG` and `*JOB` refused SQL9011 ( |
| `NEEDS-CODY` | `docs/SQL.md:546` | JTOpen `DatabaseMetaData` diff clean; ACS Run SQL Scripts checklist signed off \| `DatabaseMetaData`: met, QEMU step 132 agrees with the primary's `metadata.out` line for line; ACS checklist: |
| `BLOCKED(rel)` | `docs/SQL.md:579` | in `rpg/sqlpre/src/stmt.rs`); JDBC and the CLI read |
| `BLOCKED(rel)` | `docs/SQL.md:584` | `COLUMN_NAME`); a LOB column of a table and `WRITE_LOB` are refused SQL0084 ( |
| `BLOCKED(rel)` | `docs/SQL.md:587` | in `net/hostserver/src/database/catalog.rs`). |
| `BLOCKED(rel)` | `docs/SQL.md:589` | in `work/qsjob/src/console/drda.rs`). |
| `NEEDS-CODY` | `docs/SQL.md:590` | ACS Run SQL Scripts checklist ( |
| `NEEDS-CODY` | `docs/SQL.md:592` | ( |
| `BLOCKED(rel)` | `docs/STORAGE_SERVICE.md:395` | until each can reach its job). DFU |
| `BLOCKED(rel)` | `docs/STORAGE_SERVICE.md:447` | the service console has no job runtime). `MODE DB2SQL` after |
| `BLOCKED(rel)` | `docs/STORAGE_SERVICE.md:458` | `ALWREPCHG(*NO)` on an after trigger does not yet |
| `BLOCKED(rel)` | `docs/STORAGE_SERVICE.md:459` | refuse the program's own change of the same record ( |
| `BLOCKED(rel)` | `docs/SYSAPI.md:568` | the DRDA requester), and a CLI call made by a procedure |
| `BLOCKED(rel)` | `docs/TCPIP.md:1161` | VT and 3270 sessions and |
| `BLOCKED(rel)` | `docs/WORKMGMT.md:1249` | a program running |
| `BLOCKED(rel)` | `lic/slic/src/cpp/dbmoncmd.rs:85` | a monitor watches one job or every job. |
| `BLOCKED(rel)` | `lic/slic/src/cpp/dbmoncmd.rs:307` | the planner makes no time or storage estimate a |
| `BLOCKED(rel)` | `lic/slic/src/cpp/dbmoncmd.rs:321` | query options come from OVERRIDE_QAQQINI, not a |
| `BLOCKED(rel)` | `lic/slic/src/cpp/jrncmd.rs:58` | CMPJRNIMG's entry selection (sequence numbers, |
| `BLOCKED(rel)` | `lic/slic/src/cpp/jrncmd.rs:64` | a receiver range other than *CURRENT is not taken. |
| `BLOCKED(rel)` | `lic/slic/src/cpp/rpg/mod.rs:321` | CRTPGM does not carry a module's statements. |
| `BLOCKED(rel)` | `lic/slic/src/cpp/savcmd.rs:1450` | SAVSAVFDTA's OUTPUT(*PRINT\|*OUTFILE) listing and |
| `BLOCKED(rel)` | `lic/slic/src/cpp/sqlinfcmd.rs:86` | the plan cache (`sql::exec::plan::cache`) keeps |
| `BLOCKED(rel)` | `lic/slic/src/cpp/sqlinfcmd.rs:97` | a package's statements are kept (`db::sqlpkg`) |
| `BLOCKED(rel)` | `lic/slic/src/stgsvc/sqlhost.rs:119` | the request holds the service until it answers |
| `BLOCKED(rel)` | `lic/slic/src/stgsvc/sqlhost.rs:293` | commands whose processing program runs in the |
| `BLOCKED(rel)` | `lic/slic/src/stgsvc/sqlhost.rs:458` | the job's own variables live in the job; the |
| `BLOCKED(rel)` | `lic/slic/src/stgsvc/sqlhost.rs:542` | the lookup holds the service until the resolver |
| `BLOCKED(rel)` | `lic/slic/src/svc/db/trgout.rs:180` | the other DSPFD outfile formats (QAFDBASI, QAFDMBR, |
| `NEEDS-CODY` | `lic/svctools/src/dst/partitions.rs:9` | the primary's own panel |
| `BLOCKED(rel)` | `licpgm/lpstore/src/savfile.rs:157` | the system's own directories exist already. |
| `BLOCKED(rel)` | `net/accessweb/src/routes/db/copy.rs:699` | a file on the system is read through the Files area's trait, not built at this level; refused with XWA0099. |
| `BLOCKED(rel)` | `net/accessweb/src/routes/db/mod.rs:104` | without QS00NAV installed the link has no page; the handoff's Columns page embedded in this product is not built. |
| `BLOCKED(rel)` | `net/drdar/src/session.rs:380` | the client sends no input values (SQLDTA) yet. |
| `BLOCKED(rel)` | `net/hostserver/src/database/catalog.rs:775` | distinct types (SQLUDTS) and routine parameters |
| `BLOCKED(rel)` | `net/hostserver/src/database/mod.rs:1129` | LOB locators of the engine's own result sets |
| `BLOCKED(rel)` | `net/httpd/src/serve/handle.rs:245` | CGI programs do not run; a ScriptAlias |
| `BLOCKED(rel)` | `rpg/qrpgrt/src/sql.rs:48` | the runtime has |
| `BLOCKED(rel)` | `rpg/sqlpre/src/stmt.rs:750` | a procedure's result sets (ALLOCATE CURSOR, |
| `BLOCKED(rel)` | `runtime/sqlcli/src/exec/conn.rs:79` | a connection to another relational database |
| `BLOCKED(rel)` | `runtime/sysapi/src/job.rs:532` | a job knows the shipped descriptions only; one made or |
| `BLOCKED(rel)` | `tools/conformance/rpg/known.txt:28` | in the database's buffer conversion |
| `BLOCKED(rel)` | `tools/conformance/rpg/known.txt:32` | SQL058 a SENSITIVE cursor is refused with SQL0243: QS/400 reads a cursor's rows when it opens ( |
| `BLOCKED(rel)` | `tools/conformance/rpg/known.txt:33` | SQL060 LOB file reference variables are refused with SQL0084: the runtime reads and writes no IFS files yet ( |
| `BLOCKED(rel)` | `tools/conformance/sql/known.txt:13` | integrity/AUDITCOL   # engine: IMPLICITLY HIDDEN refused (SQL0084, |
| `BLOCKED(rel)` | `tools/conformance/sql/known.txt:29` | ARRAY_EXISTS is 7.2+ (SQL0199 on 7.1); an array parameter of a function is SQ20441 there, SQL0084 here; UNNEST of a constructor is SQL0104 there, SQ20441 here |
| `BLOCKED(rel)` | `tools/conformance/sql/known.txt:32` | in alter) |
| `BLOCKED(rel)` | `tools/conformance/sql/known.txt:35` | in udt::typing::check) |
| `NEEDS-CODY` | `tools/pesign/src/keys/mod.rs:7` | the real one is made with `pesign |
| `BLOCKED(rel)` | `ui/uim/pnlgrp/QGWCDSKS.pnlgrp:8` | headings and decimal columns sit on a hand grid this renderer does not make (work2_panels::wrkdsksts); it needs a display file with a command line |
| `BLOCKED(rel)` | `ui/uim/pnlgrp/QGWCPOOL.pnlgrp:8` | the primary's display is a display file on a hand grid this renderer does not make; it needs a display file with a command line |
| `BLOCKED(rel)` | `ui/uim/pnlgrp/QGWCSSTS.pnlgrp:10` | the primary's display is a display file on a hand grid this renderer does not make (as WRKACTJOB's); it needs a display file with a command line |
| `BLOCKED(rel)` | `ui/uim/tests/object_panels.rs:380` | the golden runs the message text through column 80 into column 1 of the next row; output fields do not wrap rows yet"] |
| `BLOCKED(rel)` | `ui/uim/tests/system2_panels.rs:273` | the golden runs the 32-character signature past column 80 into column 1 of row 18; output fields do not wrap rows yet"] |
| `NEEDS-CODY` | `ui/uim/tests/system2_panels.rs:754` | no golden; the primary has no QAUDJRN (DSPSECAUD: journal exists NO), see docs/SEC_OPEN_QUESTIONS.md"] |
| `BLOCKED(rel)` | `work/qsjob/src/command/exits.rs:131` | a program running past the wait time it asked for |
| `BLOCKED(rel)` | `work/qsjob/src/command/ftp.rs:344` | no trigger programs called here; a file with |
| `BLOCKED(rel)` | `work/qsjob/src/command/rcvjrne.rs:89` | RCVJRNE passes *TYPE1 entries one at a time from |
| `BLOCKED(rel)` | `work/qsjob/src/command/telnet.rs:50` | the TELNET client does not make TLS sessions. |
| `BLOCKED(rel)` | `work/qsjob/src/command/telnet.rs:173` | a server that will not run 5250 (VT or 3270 |
| `BLOCKED(rel)` | `work/qsjob/src/console/drda.rs:151` | the requester's query brings no column descriptions |
| `BLOCKED(rel)` | `work/qsjob/src/console/jobcmd.rs:151` | another job's call stack needs a request the |
| `BLOCKED(rel)` | `work/qsjob/src/devtools/isql/prompts.rs:347` | F6, F10, F11, F14, F15, F19, F20 and F24 of |
| `BLOCKED(rel)` | `work/qsjob/src/devtools/isql/prompts.rs:458` | the WHERE display after this one is not |
| `BLOCKED(rel)` | `work/qsjob/src/qapsql/mod.rs:354` | QAP's Executed has no output values (a later |
| `BLOCKED(rel)` | `work/qsjob/src/qapsql/mod.rs:421` | `Executed` names one cursor, so a second set is not |
| `BLOCKED(rel)` | `work/qsjob/src/work/httpafw/cmd.rs:337` | the bridge waits 120 seconds for the job's thread; a longer command |
| `BLOCKED(rel)` | `work/qsjob/src/work/httpafw/msgs.rs:40` | the sender is the qualified job; a message a user typed here names |
| `BLOCKED(rel)` | `work/qsjob/src/work/httpafw/print.rs:192` | no command or service request a web job may use puts a spooled file at the top of a queue (QAP's move does, on its own connection job only); refused with XWA0099. |
| `BLOCKED(rel)` | `work/qsjob/src/work/httpafw/s5250.rs:314` | a session to another system goes through that system's own Access for Web; refused with QAD1002. |
| `BLOCKED(rel)` | `work/qsjob/src/work/httpafw/s5250.rs:669` | the display shows and types CCSID 37; the session's CCSID goes to the server as CODEPAGE only. |
| `BLOCKED(rel)` | `work/qsjob/src/work/httpafw/system.rs:186` | the password approval program (QPWDVLDPGM) is not called from a web job; refused with CPF22D3 while one is set. |
| `BLOCKED(rel)` | `work/qsjob/src/work/httpsvr.rs:67` | validation lists (*VLDL) do not exist; a PasswdFile |
<!-- inventory:end -->
