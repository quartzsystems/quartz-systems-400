# Save and restore facts from the primary oracle

Captured on 2026-10-30 from the primary oracle (IBM i V7R1M0, profile
`QSECOFR`) for Phase 11. Every save and restore ran against library
`QSTEST` and `/home/QSTEST` only, into save files inside `QSTEST`; no
library was restored under another name. The system name is written
`{host}` and the serial number `{serial}`.

## How

The remote command server (8475) drops every connection at the seed
exchange on this box today, so everything ran over JDBC (8471): CL through
`CALL QSYS.QCMDEXC`, queries through SQL. The tool and its scripts are
in `../tool/`. After every command the tool prints the job log
(`DSPJOBLOG OUTPUT(*PRINT)`, copied back with `CPYSPLF` by a small CL
program that uses `QSPRILSP`) and keeps the messages not seen before.
Each command is written as

```
> COMMAND
  -> ok | FAILED [ESCAPE-ID] first-level text
  MSGID TYPE SEV FROM-PGM->TO-PGM | first-level text
```

`TYPE` is the job log's message type (Completion, Diagnostic,
Information, Escape, Sender copy), `SEV` its severity. Message data in
hex is not available this way; the first-level text carries the
substituted values. `-- spooled file (NAME)` blocks are spooled output
copied back (`OUTPUT(*PRINT)`, or the batch print of a display
command). `-- TITLE: SQL` blocks are query results; the outfiles of
`SAVLIB`/`RSTOBJ OUTPUT(*OUTFILE)` are CCSID 65535, their character
columns were decoded from EBCDIC afterwards, and the one-byte flags left
in hex read `F0` = `0`, `F1` = `1`.

Two caveats on attribution. Messages an SQL statement, a QSH command or
a `splf` display sent show under the next `>` command in cases 01 to 18
(the tool flushed them only from the follow-up runs on); they are easy
to tell apart (`SQL....`, `QSH0005`, `CPCA081`, `CPCA089`, `CPF4271`).
Case 04's first lines (`CPF7080`, `CPI3203`, `CPC3722 31 objects`)
belong to case 03's last `SAVLIB ... OUTPUT(*PRINT)`.

The inquiry reply was `CHGJOB INQMSGRPY(*DFT)` in every job, so an
inquiry (`CPA4067`) shows as a sender copy with its default reply.

## State

| | |
|---|---|
| Before | `QSTEST` held no objects; `/home/QSTEST` held the 2026-10-22 kit folders `data`, `dds`, `oracle`, `out` (`00-survey.txt`) |
| Objects | built by `01-build.txt`: DDS PFs `CUST`, `ORD` (journaled to `JRN`, receivers `RCV0001`, `RCV0002`), `FMTA`, `FREEPF`, multi-member `MPF` (`M1`..`M3`); LFs `CUSTK` (keyed), `CUSTSO` (select/omit), `CUSORD` (join); source PFs `QSRC`, `QDDSSRC`; SQL table `SQLT` with index `SQLTIX`; data areas `DTAC` (*CHAR), `DTAD` (*DEC, owner `OWNA`, *PUBLIC *EXCLUDE), `DTAL` (*LGL, group `GRPA`); `DQ1` with one entry; `USRSPC`, `USRIDX`; `QSTMSGF`; CL `PGM1`, modules `MOD1`/`MOD2`, `SRV1` from binder source, `PGM2` bound to it; `QSTCMD` over `PGM1`; `QSTJOBD`; `QSTOUTQ` with two spooled files; private authorities for `USRA` on `CUST` (*CHANGE), `DTAC` (*USE), `PGM1` (*EXCLUDE). `ZZ*` objects are the tool's (omitted from every save but the first) |
| Save file | `ZZFULL`: `SAVLIB QSTEST OMITOBJ(ZZ*) UPDHST(*YES)` from case 02, used by every restore |
| After | see the top-level README |

## Cases

| File | What it settles |
|---|---|
| `00-survey.txt` | `QSTEST` and `/home/QSTEST` before; the profiles `USRA`, `OWNA`, `GRPA` used for authority cases |
| `01-build.txt` | the objects, their authorities, `WRKJRNA`, `DSPLIB`, the output queue |
| `02-savlib.txt` | `SAVLIB` with the target save file inside the library; `DSPSAVF` in batch and `OUTPUT(*PRINT)`; `DSPOBJD` and `DSPFD` of a save file |
| `03-savlib-outfile.txt` | `OUTPUT(*OUTFILE)` with `INFTYPE(*OBJ)`, `*MBR`, `*LIB`, `*ERR` (the `QASAVOBJ` format: `DSPFFD` and rows); `OUTPUT(*PRINT)` listings for `*OBJ` and `*MBR` |
| `04-savobj.txt` | `SAVOBJ` of a subset, `FILEMBR`, `QDTA(*DTAQ)`, `SPLFDTA(*ALL)`, objects not found, a missing library, a type list |
| `05-updhst.txt` | `UPDHST(*NO)`/`(*YES)` on `DSPOBJD`'s save fields; the `QADSPOBJ` save and restore columns of every object |
| `06-savchgobj.txt` | `SAVCHGOBJ REFDATE(*SAVLIB)`, an explicit `REFDATE`/`REFTIME`, `OBJJRN(*YES)` |
| `07-clear.txt` | `CLEAR(*NONE)`/default, `*REPLACE`, `*ALL`, `*AFTER` on a save file; an empty save file; a save file too small (`MAXRCDS`); `DTACPR`; `TGTRLS` |
| `10-rstlib-over.txt` | `RSTLIB` and `RSTOBJ *ALL` over an existing library |
| `11-option.txt` | `OPTION(*NEW)`, `(*OLD)`, `(*FREE)` with `STG(*FREE)` |
| `12-mbropt.txt` | `MBROPT(*MATCH)`, `*NEW`, `*OLD`, `*ALL` with members removed and added; `FILEMBR` on restore; a deleted file |
| `13-alwobjdif.txt` | `ALWOBJDIF` with a file recreated with another format and with the same format, a changed owner, a changed primary group; the list form (two runs, the second with `MBROPT(*ALL)`) |
| `14-locked.txt` | restores and a save over objects another job allocated; a lock held by the restoring job; `ALCOBJ` of a save file |
| `15-journal.txt` | a journaled file restored with the journal present, over itself after `ENDJRNPF`, with the journal deleted, after the journal is restored; a deleted receiver restored; an existing receiver and journal; `DSPJRN` |
| `16-authority.txt` | private authorities, owner, primary group and `*PUBLIC` after restoring over an existing object and after restoring a deleted one |
| `17-errors.txt` | objects not in the save file, the wrong library or type, an empty or missing save file, a file that is not a save file, `OMITOBJ`; `RSTOBJ OUTPUT(*OUTFILE)` (the `QASRRSTO` format) and `OUTPUT(*PRINT)` |
| `18-dependents.txt` | logical files restored without their physical file and in one request with it; an SQL index |
| `19-ifs.txt` | `SAV` and `RST` of `/home/QSTEST` and a subtree: counts, `OPTION(*NEW/*OLD/*ALL)`, a new name, an omitted subtree, not found, `OUTPUT(*PRINT)`, a QSYS path on `RST` |
| `30`..`35` | Part D, read-only: `DSPSFWRSC`, `DSPPTF` (all products, 5770SS1, 5770999, one PTF), `DSPPTFCVR`, PTF groups, `DSPIPLA`, `DSPBCKUP`, `DSPBCKUPL`, `WRKCFGSTS *DEV`, `DSPDEVD` of the tape and optical devices, `DSPTAP`, `DSPOPT`, the image catalog, `DSPPRB`, `WRKLICINF`, `DSPLICKEY` |

## Restore rules settled

Completion and counting:

1. A restore that restores everything ends with `CPC3703` "n objects
   restored from LIB to LIB"; with `FILEMBR` it is `CPC3727` "n objects
   restored. 0 objects excluded". If any object is not restored the
   command ends with escape `CPF3773` (severity 20) "n objects restored.
   m not restored to LIB", each failure having its own diagnostics
   first. If nothing is restored the escape is `CPF3770` (30) "No objects
   saved or restored for library". A harder escape (`CPF3706`) wins and
   `CPF3773` is then sent as a diagnostic (`17`).
2. Objects not selected by `OPTION` are informational, not failures:
   `CPF3725` "not selected" for objects that exist under `*NEW`,
   `CPF3724` for objects missing under `*OLD`, `CPF3726` for objects
   whose storage was not freed under `*FREE`; they are not counted in
   `CPF3773` (`11`).
3. Each restored database file sends `CPF3292` (info 10) "File F in
   library L restored" and `CPI3203` "n logical access paths saved or
   restored"; a restored object that is older than the one it replaced
   sends `CPF3809` (info 10) "Older version of X type T restored" (only
   the first restore over the changed objects, `10`).
4. Security or format differences accepted under `ALWOBJDIF` add
   `CPF3848` (diag 20) "n security or data format changes occurred" and
   turn the completion into `CPF3773 ... 0 not restored` (`13`, `15`).
5. Save side: `CPC3722` "n objects saved from library", `CPC3723` "n
   objects saved. m objects not included" (`FILEMBR`, `TGTRLS`, and
   `SAVCHGOBJ` with `OBJJRN(*NO)` leaving journaled files out),
   `CPF3701` (escape 20) "n saved, m not saved" with `CPF3764` per name
   not found, `CPF3770` when none, `CPF3781` for a missing library
   (`04`, `06`).

What a restore will not replace:

6. An existing journal: `CPF7085` + `CPF3756` (`10`, `15`).
7. An existing data queue: `CPD2415` (40) + `CPF3756`; data queues are
   never restored over (`10`).
8. An attached receiver: `CPF7081` + `CPF3756`; an existing detached
   receiver: `CPF3734` + `CPF3706` escape (`10`, `15`).
9. An object another job allocated (`*EXCL`, `*SHRRD`, `*EXCLRD`, any
   type tried): `CPF3761` (diag 20) "Cannot use T NAME in LIB", counted
   as not restored. A lock held by the restoring job does not block the
   restore. A save of an object another job holds `*EXCL` fails the same
   way (`14`).
10. A logical file whose physical file is missing: `CPF3204` (40) +
    `CPF3756`. Logical files named before their physical file in one
    request are restored after it (all four restored) (`18`).

Members (`MBROPT`, default `*MATCH`):

11. `*MATCH` with a different member list: `CPF3284` (diag 50) "Member
    list of file F and saved file not the same" + `CPF3756`. `*NEW`
    adds only saved members missing on the system; `*OLD` restores only
    saved members that exist (others not added); `*ALL` restores every
    saved member and keeps members the save did not have. `FILEMBR` is
    refused with `*MATCH` (`CPD3759`). A deleted file comes back with
    exactly its saved members (`12`).

Object differences (`ALWOBJDIF`, default `*NONE`):

12. A file whose level identifiers differ is refused with `CPF3283`
    (diag 50) "Saved file or member level ID not same as file F" under
    `*NONE`, `*FILELVL` and `*COMPATIBLE` when the format differs, and
    under `*NONE` even when only the creation (the member level ID)
    differs. `*FILELVL` restores over a same-format file recreated
    later. `*ALL` needs `MBROPT(*ALL)` (`CPD3768`) and must be the only
    value (`CPD0094`); with a different format it renames the existing
    file to `FMTA0001`, sends `CPI320A` "File level identifiers do not
    match" and restores the saved file with its saved creation date
    (`13`).
13. Owner differs: `*NONE` refuses with `CPF3766` (diag
    20) "T NAME not restored to library"; `*OWNER` (or `*ALL`) restores
    and keeps the existing owner, sending `CPF370B` "USER owns T NAME in
    LIB". Primary group differs: `*NONE` refuses with `CPD37A2`;
    `*PGP` restores, keeps the existing group and sends `CPI3811` "*NONE
    is the primary group of ..." (`13`).

Authorities without `RSTAUT` (`16`):

14. Restored over an existing object: the existing object's private
    authorities stay (including grants made after the save), its owner
    stays, and `*PUBLIC` is set back to the saved value.
15. Restored as a new object: no private authorities at all (the saved
    ones are gone); the owner, `*PUBLIC` (`*EXCLUDE` kept) and the
    primary group are the saved ones.

Journaling (`15`):

16. A journaled file deleted and restored while its journal exists is
    journaled again (same images; last start time = restore time).
17. A file restored over an existing file that is not journaled stays
    not journaled.
18. Journal missing: the file is restored, not journaled: `CPF9801`
    "Object JRN in library L not found" and `CPF3294` "Cannot start
    journaling for member *N" (info 40), then `CPF3848` and `CPF3773
    1 objects restored. 0 not restored`.
19. Restoring a deleted journal creates and attaches a new receiver
    (`CPC7011` "Journal receiver RCV1002 created", the number 1000 past
    the last) and leaves the old receivers out of its chain until
    restored; files restored after it are journaled again. A deleted
    receiver restored rejoins the directory (`WRKJRNA`).

Save files:

20. A non-empty save file without `CLEAR`: inquiry `CPA4067` "already
    contains data. (C G)", default C: `CPF4110` + `CPF3794`.
    `CLEAR(*ALL)` and `CLEAR(*REPLACE)` overwrite; `CLEAR(*AFTER)` is
    refused with a save file (`CPD3754`). Full save file: `CPF3808` +
    `CPF3815`. Empty: `CPF3707` on restore. Missing: `CPF9812`. Not a
    save file: `CPF3782` (`07`, `17`).
21. The save file being written, when inside the saved library, is saved
    without its data (`CPI374B` "SAVFDTA(*YES) ignored") and counted;
    other save files are saved with data (`02`).
22. An attached receiver is saved with `CPF7080` "saved while attached"
    (`02`).
23. `DSPSAVF` in a batch job prints without `OUTPUT(*PRINT)`. Its order
    is the save order: the library, journals, programs, service
    programs, modules, data queues, output queues, message files, user
    indexes, save files, job descriptions, commands, data areas, user
    spaces, database files (physical before their logical files), then
    receivers (`02`).
24. Not found on restore: `CPF3801`/`CPF3806` (diag 20) + `CPF3780`
    "Specified file for library L not found"; one of several names not
    found: `CPD3773` "Object NAME type *ALL in L not restored" (`17`).
25. IFS: `SAV` ends `CPC370D` "n objects saved"; `RST` ends `CPC370F` "n
    objects restored", `CPC3716` when `*NEW` excluded everything,
    `CPF3801` + `CPF3840` when the path is not in the save file;
    `*INCLUDE` with a new name restores under it; `RST` takes a QSYS
    path from a library save file; `RSTOBJ` from an IFS save file fails
    `CPD372A` + `CPF3780` (`19`).
26. `UPDHST(*NO)` leaves the save date, command, device and save file of
    `DSPOBJD` unchanged; `*YES` sets them. `REFDATE(*SAVLIB)` uses the
    library's last `SAVLIB UPDHST(*YES)`; `SAVCHGOBJ UPDHST(*YES)` does
    not move it (`05`, `06`).
27. On V7R1M0 with this PTF level `SPLFDTA(*ALL)` and `QDTA(*DTAQ)` are
    accepted; `TGTRLS(V6R1M0)` is accepted, `V5R1M0` refused (`CPD0084`)
    (`04`, `07`).

## Not settled

- `*SAVF` restore needing `*USE` on the save file: every job here runs
  as `QSECOFR` (`*ALLOBJ`), and no other profile may be signed on.
- An owner profile that does not exist on the target (needs a profile
  deleted or a save from another system).
- Whether `SPLFDTA(*ALL)` actually saved the two spooled files of
  `QSTOUTQ` (the save file was cleared before `DSPSAVF`; see the
  top-level README for the last run).
- `RSTAUT`, `RSTUSRPRF`, `SAVSECDTA`, `SAVSYS`, `SAVCFG`, tape and
  optical media: forbidden or not loaded (`DSPTAP`: `CPF6760` device
  not ready; both optical devices empty, `OPT1660`).
- Message data in hex: the JDBC route gives first-level text only.
- `WRKPTFGRP`, `WRKPRB`, `WRKDEVD`, `WRKHDWRSC`, `WRKIMGCLGE`,
  `DSPSRVSTS` are not allowed in a batch job (`CPD0031`) and `DSPSRVA`
  needs a display; `QSYS2.GROUP_PTF_INFO` returns no rows; no PTF on
  the box has a cover letter (`CPF35F5`, `CPF6613`).
