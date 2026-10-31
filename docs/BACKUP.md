# Cleanup and backup

The Operational Assistant's two jobs: removing what the system no longer
needs (`GO CLEANUP`), and saving it on a schedule (`GO BACKUP`, `GO SAVE`
and `GO RESTORE` options 20 to 23). The options are kept as `key=value`
text in user spaces in `QUSRSYS`; the model is `work/wm/src/opa/`, the
service's commands `lic/slic/src/cpp/opacmd.rs`, the job's side
`work/qsjob/src/panels/backup.rs`.

| Object | Holds | Model |
|---|---|---|
| `QUSRSYS/QEZCLNOPT` | the cleanup options, the last run and what it removed | `opa::clnup::Cleanup` |
| `QUSRSYS/QEZBCKOPT` | the daily, weekly and monthly backup options, last run and tape set | `opa::bckup::Backup` |
| `QUSRSYS/QEZBCKHST` | the backups that ran, newest last, 100 kept | `opa::hist::History` |
| `QUSRSYS/QEZSAVDFT` | the `GO SAVE` command defaults (option 20) | `opa::gosave::Defaults` |

## Cleanup

`CHGCLNUP` changes the options; every parameter defaults to `*SAME`, and
the prompt shows the current values through the prompt override
(`PMTOVRPGM`, answered by the hidden `QSPMTOVR`). Shipped values:

| Keyword | Shipped | Kept for |
|---|---|---|
| `ALWCLNUP` | `*YES` | whether `STRCLNUP` runs at all |
| `STRTIME` | `22:00:00` | the schedule entry's time; `*SCDPWROFF` adds none |
| `USRMSG` | 7 | messages on the queues in `QUSRSYS` |
| `SYSMSG` | 4 | messages on `QSYSOPR` and the other queues in `QSYS` |
| `CRITSYSMSG` | `*KEEP` | messages of severity 90 or more on those |
| `SYSPRT` | 7 | spooled files on `QUSRSYS/QEZJOBLOG` and `QEZDEBUG` |
| `SYSLOG` | 30 | `QHST` files, detached receivers of the journals in `QSYS` (`QAUDJRN` among them), problems |
| `CALITM` | 30 | office calendar items (none on QS/400) |
| `JOBQ` | `*LIBL/QCTL` | where the cleanup job goes |
| `JRNRCVSIZ`, `RUNPTY` | 100000, 55 | kept, shown |

`STRCLNUP OPTION(*SCHED)` adds job schedule entry `QEZCLNUP`: weekly on
every day at `STRTIME`, on `JOBQ`, as `QPGMR`, running `STRCLNUP
OPTION(*IMMED)`. `CHGCLNUP` moves the entry; `ENDCLNUP` removes it.
`OPTION(*IMMED)` runs one cleanup in the service:

1. spooled files on `QEZJOBLOG`/`QEZDEBUG` older than `SYSPRT` (not
   open, not printing);
2. `QSPLMAINT`'s pass (`QRCLSPLSTG`): expired spooled files; a spooled
   file's storage goes with it, so nothing more is orphaned;
3. messages past `USRMSG`/`SYSMSG`/`CRITSYSMSG`; an inquiry waiting for
   its reply is never removed;
4. `QHST` files whose day is past `SYSLOG`, never the newest;
5. detached (`*ONLINE` or `*SAVED`) receivers of every journal in `QSYS`
   detached more than `SYSLOG` days ago;
6. problems opened more than `SYSLOG` days ago;
7. every object in `QRPLOBJ` but those a temporarily applied PTF still
   needs to be removed.

The run's summary goes to `QHST` (`CPC1E23`) and into the options.

## Backup

`CHGBCKUP BCKUPOPT(*DAILY|*WEEKLY|*MONTHLY|*ALL)` changes one backup's
options (`DEV`, `TAPSET`, `CLRTAP`, `SBMJOB`, `CHGONLY`, `PRTRPT`, `LIB`,
`FLR`, `DIR`, `SECDTA`, `CFG`, `MAIL`, `CAL`, `EXITPGM`). `DSPBCKUP`
shows them: the `QEZBCKUP` panels at a display (`DSPBCKD`, `DSPBCKW`,
`DSPBCKM`, matched with `golden/phase11/displays/dspbckup*`), text
otherwise.

`RUNBCKUP BCKUPOPT() DEV()` runs in the job. The service answers its
plan (`QSBCKUP ACTION(*PLAN)`): the device, the next tape set of the
rotation, and the saves in order:

1. `SAVSECDTA` when `SECDTA(*YES)`;
2. `SAVCFG` when `CFG(*YES)`;
3. `SAVLIB LIB(...)`, or `SAVCHGOBJ OBJ(*ALL) LIB(...)` with
   `CHGONLY(*YES)`: `*ALLUSR`, or the user libraries for `*BCKUPL`;
4. `SAV` of every directory but `/QSYS.LIB` and `/QDLS` when `DIR` is
   not `*NONE`.

The first save clears the tape when `CLRTAP(*YES)`; the others add
`SEQNBR(*END)`, and all but the last `ENDOPT(*LEAVE)`. `LIB(*ALLUSR)`
needs the restricted state: an interactive job shows the commands
(`SAVCMDS`) and asks for Enter, then `ENDSBS SBS(*ALL) OPTION(*IMMED)`,
waits for the subsystems to end (five minutes at most), runs the saves
and `STRSBS` of `QCTLSBSD`. Otherwise, with `SBMJOB(*YES)` an
interactive `RUNBCKUP` submits itself to `QSYS/QBATCH` as job
`QEZBCKUP`. Each run is recorded (`QSBCKUP ACTION(*DONE)`): the history,
the last date and tape set, `CPC1E31` or `CPF1E32` to `QHST`.
`DSPBCKSTS` lists the history.

`GO BACKUP` option 1 is menu `RUNBCKUP` (the three backups, and option
11 for `GO SAVE 21`), option 10 menu `SETUPBCKUP` (the three
`CHGBCKUP` prompts, the daily display, a schedule entry for the daily
backup).

## GO SAVE and GO RESTORE

Options 20 to 23 of `GO SAVE` and 21 to 23 of `GO RESTORE` run
`QSMNSAVE`/`QSMNRST OPTION(n)` in the job. Each shows the "Specify
Command Defaults" panel (`SAVDFT`: devices, prompt for commands, check
for active files, message queue delivery, start time, vary off network
servers, unmount file systems, print system information, use system
reply list, spooled file data); option 20 keeps the values, the others
run with them (`opa::gosave::sequence`):

| Option | Commands |
|---|---|
| SAVE 21 | `ENDSBS *ALL`, `CHGMSGQ QSYSOPR`, [`PRTSYSINF`], `SAVSYS`, `SAVLIB *NONSYS`, `SAV` all directories, `STRSBS` |
| SAVE 22 | the same with `SAVLIB *IBM` and `SAV` of `/QIBM/ProdData` |
| SAVE 23 | `ENDSBS *ALL`, `CHGMSGQ`, `SAVSECDTA`, `SAVCFG`, `SAVLIB *ALLUSR`, `SAV` user directories, `STRSBS` |
| RESTORE 21 | `ENDSBS *ALL`, `CHGMSGQ`, `RSTUSRPRF`, `RSTCFG`, `RSTLIB *NONSYS`, `RST`, `RSTAUT`, `STRSBS` |
| RESTORE 22, 23 | the same with `*IBM` or `*ALLUSR` and their directories |

A start time other than `*CURRENT` puts `DLYJOB RSMTIME` first; the
reply list adds `CHGJOB INQMSGRPY(*SYSRPYL)`; prompt for commands puts
`?` before each save. A failed save does not stop the next; `STRSBS`
always runs. Commands this release does not ship (`SAVDLO`, `UNMOUNT`,
network server `VRYCFG`) are left out. Because `ENDSBS *ALL` ends every
job in a subsystem, these options and a restricted `RUNBCKUP` refuse
(`CPF1E36`) in a job that runs in one: they run from the console job,
`QSVCJOB`, which is outside the subsystems.

## PRTSYSINF

Spooled file `QPRTSYSINF` through `QSYSPRT`: the text of `WRKSYSVAL
OUTPUT(*PRINT)`, `DSPNETA`, `WRKCFGSTS CFGTYPE(*DEV)`, `DSPAUTUSR`,
`DSPSFWRSC`, `DSPPTF`, `DSPLIBL`, each under its heading (a command that
cannot answer in the service leaves its message instead), and the
startup program `QSTRUP` as shipped.

## Deviations and what waits

- `GO SAVE` 21 and 22 run `SAVSYS` when the system has it, and save
  everything after it otherwise.
- The backup list (`DSPBCKUPL`, `EDTBCKUPL`) is not there: `*BCKUPL`
  stands for every user library; F16 and F17 of `DSPBCKUP` say so.
- `RTVCLNUP` and `RTVBCKUP` are not shipped.
- `ipl_after` (power off and IPL after a backup) is kept in the options
  and acted on by `RUNBCKUP`, but no panel sets it yet.
- The job that ends the subsystems is not kept alive as IBM keeps the
  console's job in the controlling subsystem.
