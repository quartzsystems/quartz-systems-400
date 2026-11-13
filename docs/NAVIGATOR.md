# Navigator for QS/400

The system management web interface: HTTP server instance `NAVIGATOR`
in subsystem `QHTTPSVR` on port 2002 and, with TLS, 2005. The approved
design is the handoff (`docs/NAVIGATOR_HANDOFF.md`), the style standard
`docs/WEB_STYLE.md`, and the golden pages under
`net/navigator/tests/golden/`.

## Crate

`net/navigator` runs over [`qsweb`](HTTPADMIN.md) unchanged, as HTTP
Server Administration does.

| Module | What |
|---|---|
| `env/` | `Env`: `System` (name, clock, sign-on, special authorities, audit, message texts, System status) and one trait per area: `work`, `msgs`, `users`, `config` (system values, disks, hardware, time), `net`, `db`, `sec` (audit journal), `sw` (licensed programs, PTFs), `perf`. |
| `routes/` | `dispatch` (session, CSRF, the `/nav` tree toggle), `Rq`, sign-on, home, the generic pages, then one folder per area routing its own first path segments. |
| `fixture/` | The prototype's sample system: `QSHQ400`, clock 10/29/26 14:03:40, one module per area, `text.rs` the message texts its pages show. |
| `help.rs` | Help topics. |

`cargo run -p navigator --bin navigator-demo [PORT]` serves the product
over the fixture at `http://127.0.0.1:2002/qs400/navigator/` (any
fixture profile, password `QS400`).

## Decisions

- Every page is the HTML form of a command or API a 5250 user has, run
  as the signed-on user, never a private path into the object store:
  what Navigator shows is what the command shows, and the command
  checks its own authority. Navigator owns no messages.
- Any profile the system accepts signs on (5250 rules, `PW` entries);
  each page or action then needs what its command needs. A refusal is
  the Not authorized page with the command's message (`CPF2207` from
  `Rq::require`) and an `AF` entry.
- Routes: `/qs400/navigator/` + the prototype's file name as a path
  (`job-props-joblog.html` is `/job/<number>/<user>/<name>/joblog`).
  My messages is `/mymsgs` (the user's own queue); QSYSOPR is
  `/msgq/QSYS/QSYSOPR`. Security values is `/sysval?cat=security`.
- The certificate application ID is `QS400_HTTP_SERVER_NAVIGATOR`.
- Auto-refresh offers Off, 30, 60, 120 and 300 seconds; the
  `<meta http-equiv="refresh">` is there only when one is chosen. The
  home page's elapsed interval runs from the previous display.
- The tree (`qsweb`): `[+]` and `[-]` open and close a group in place
  (`/nav?open=G&back=URL`); the groups a user opened stay open for the
  session and arriving at a page opens its group. The golden test maps
  a toggle link to the group's first page, which is where the
  prototype's glyphs point.

## Work management

`env::work::Work` reads what the `WRK*`/`DSP*` commands show and runs
every change as a CL command through `Work::run(user, command)`: the
page builds the command (and shows it), the live job runs it through
the analyzer as the signed-on user, so the command checks its own
authority and its completion or escape message is what the next page
shows. Navigator additionally checks the special authority the command
needs where it is known before running it (`*JOBCTL` for another
user's job, job queues, subsystems, output queues, writers, devices,
another user's schedule entries; another user's spooled file needs
`*JOBCTL` unless the user has `*SPLCTL`), so a refusal is the Not
authorized page and an `AF` entry.

- Every row action that changes something (Hold, Release, End, Start,
  Clear, Delete, Vary on/off, Submit now ...) is a GET page naming the
  command and its objects (`Confirm Hold Job`, ...) whose OK posts; the
  dangerous ones say what is lost. Nothing changes on a GET.
- Routes: `/jobs`, `/job/<number>/<user>/<name>/{general,performance,
  liblist,joblog,splf,locks,callstack,hold,release,end}`, `/jobq`,
  `/jobq/<lib>/<name>/{hold,release,confirm}` (confirm = Clear), `/sbs`,
  `/sbs/<name>/{general,pools,routing,autostart,workstation,prestart,
  start,end}`, `/sbs/<name>/{routing,autostart}/confirm?seq=|job=`,
  `/outq`, `/outq/<lib>/<name>[/hold|/release|/confirm|/props]`,
  `/splf/<number>/<user>/<job>/<file>/<nbr>[/pdf|/text|/hold|/release|
  /confirm|/move]`, `/writers`, `/writers/<dev>[/start|/hold|/release|
  /varyon|/confirm?op=end|varyoff]`, `/jobscde`, `/jobscde/add`,
  `/jobscde/<name>/<entry>/{change,hold,release,submit,confirm}`.
- Active jobs: the figures line (elapsed interval, CPU %, active jobs,
  `MSGW`) shows above a non-empty list when the last action did not
  fail; `ActiveJobs::notes` are messages about jobs the user ended that
  have finished since the last look (the golden `CPF1321` row), shown
  once. The Subsystem select and the job and output queue Library
  selects offer `Work::choices`.
- Selects mark the chosen option `selected` unless it is the first (the
  default), as the prototype does.
- Spooled files: the view is one page of the file's text in a
  `<pre class="out out132">`, never reflowed; Find searches from the
  page shown to the end, then from page 1 (`CPI9801` when absent).
  PDF is `spool::pdf` over the pages.
- Pools, routing and autostart entries are edited in a form under their
  list (`?edit=`, `?seq=`, `?job=`): `CHGSBSD POOLS()`,
  `ADDRTGE`/`CHGRTGE`, `ADDAJE`/`CHGAJE`; Remove is confirmed
  (`RMVRTGE`, `RMVAJE`).
- `shared/confirm.html` is HTTP Server Administration's Confirm Delete
  Instance; no Navigator page carries that data. The work management
  confirmations are built the same way (sentence, table, options, the
  equivalent command, OK and Cancel) and the crawler maps them to it.

## Messages

`env::msgs::Msgs` reads what `DSPMSG` and `DSPLOG` show and runs
every change through `Msgs::run(user, command)`: Send is `SNDMSG`
(`TOUSR` or `TOMSGQ`; an inquiry's `RPYMSGQ` is the user's own queue),
Reply is `SNDRPYMSG MSGKEY() MSGQ() RPY() RMV(*NO)` (the system's name
for the reply command; `RMV(*NO)` keeps the answered inquiry on the
queue with its reply, as `DSPMSG` shows it), Remove is `RMVMSG`, Clear
queue is `CLRMSGQ CLEAR(*KEEPUNANS|*ALL)`. Remove and Clear are
confirmed; an answered inquiry is refused by the command (`CPF2422`).

- Routes: `/msgq/<lib>/<name>` the operator's view of any queue (Queue,
  Type and Since filters, Clear queue...), `/msgq/<lib>/<name>/<key>`
  the detail and Reply, `.../<key>/remove`, `/msgq/<lib>/<name>/clear`,
  `/mymsgs` the user profile's queue with Send message, `/qhst`. Links
  from My messages carry `?from=my` so their pages return there.
- The operator's view cuts text at 70 characters and links the sending
  job when it is still in the system (`Work::job`); My messages shows
  whole texts and the job as text, as the prototype does. A message a
  user typed shows as `From USER: text`; Send's To user starts as the
  sender of the newest one.
- An empty reply sends the inquiry's default (`RPY(*DFT)`), as Enter on
  an empty reply does at the 5250 display.
- Second-level text shows command strings in it in Courier
  (`DSPRCDLCK FILE(...) ...`): a capitalised name followed by
  `KEYWORD(value)` parameters.

## Users and groups

`env::users::Users` reads what `WRKUSRPRF`, `DSPUSRPRF`, `WRKAUTL`,
`DSPAUTL` and `DSPAUTLOBJ` show; every change runs through
`Users::run(user, command)`: Apply is one `CHGUSRPRF` with only what
changed, Reset password `CHGUSRPRF PASSWORD() PWDEXP()`, Delete
`DLTUSRPRF OWNOBJOPT(*NODLT|*DLT|*CHGOWN user)` (the confirmation asks),
Create group `CRTUSRPRF PASSWORD(*NONE) GID(*GEN)`, authorization lists
`CRTAUTL`, `DLTAUTL`, `ADDAUTLE`, `CHGAUTLE`, `RMVAUTLE`. The profile
commands need `*SECADM` (checked first, the Not authorized page and an
`AF` entry); the commands check the rest (special authorities you do
not have, shipped profiles, groups with members, owned objects).

- Routes: `/users`, `/user/<name>/{general,groups,authorities,personal,
  password,delete}` (Jobs is `/jobs?user=`), `/users/create/step/<n>`,
  `/groups`, `/groups/create`, `/autl`, `/autl/create`,
  `/autl/<name>[?edit=USER]`, `/autl/<name>/{objects,remove?user=,
  delete}`.
- Users lists user profiles; group profiles (profiles with a GID that
  others name as their group) are on Groups, and their Properties and
  Delete are the profile's pages. The Group filter and Members match
  primary and supplemental groups.
- The Create user wizard keeps its answers in the session under `cu.`:
  Create user... (a POST) starts it blank, Back and Next keep them,
  Cancel and Create forget them; a step not reached yet sends you to
  the furthest one. The summary prints the command with the password
  left out (`PASSWORD()`, or `PASSWORD(*NONE)` when none was typed),
  wrapped at 90 columns. Also creates the home directory (`CRTDIR`,
  then `CHGOWN` to the new user) and the message queue (`CRTMSGQ
  QUSRSYS/<name>`) when asked.
- Sign-on stays the fixture's four profiles (`fixture::PROFILES`:
  QSECOFR, QSYSOPR, QPGMR, WEBUSER); the Users fixture's fifteen take
  the special authorities of those they share from that list, and the
  security commands check the signed-on user's from it. QSYSOPR signs on
  but is not among the prototype's fifteen.
## System, network, performance

`env::config::Config` (system values, disks, hardware, time),
`env::net::Net` (TCP/IP and the servers) and `env::perf::Perf` read
what `WRKSYSVAL`, `WRKDSKSTS`, `WRKHDWRSC`, `CFGTCP`, `NETSTAT`,
`WRKSYSACT` and `WRKSYSSTS` show; changes run through `Config::run`
and `Net::run` as the user, as in Work management.

- Routes: `/sysval?cat=<key>&sort=n` (keys `security`, `datetime`,
  `jobs`, `storage`, `msgs`, `devices`, `libl`, `perf`, `power`, `save`,
  `sysctl`; none is Security under the System group, `cat=security` is
  the Security group's Security values), `/sysval/<name>`, `/disk`,
  `/disk/<unit>`, `/hdwrsc?t=&s=`, `/hdwrsc/<name>[/varyon|/confirm]`,
  `/time`, `/tcp`, `/tcp/hosts`, `/tcp/ifc[?add=1|?edit=<addr>]`,
  `/tcp/ifc/{start,confirm}?a=<addr>[&op=end|remove]`,
  `/tcp/rte[?add=1|?d=&m=&n=]`, `/tcp/rte/confirm?d=&m=&n=`,
  `/tcp/servers[/<name without *>[/start|/confirm?op=end|restart]]`,
  `/perf/sys?a=&sort=cpu|io`, `/perf/disk?a=`.
- Change system value checks the value against what the value accepts
  (the grey hint) before `CHGSYSVAL` runs; a refused value stays on the
  page with `CPD9B8D`. Security values need `*SECADM`. The command
  writes the `SV` entry.
- Disk status's Reset is `WRKDSKSTS RESET(*YES)`: it restarts the
  user's own I/O counters, changes nothing on the system, so it is a
  plain link (`/disk?reset=1`) with no confirmation. Bars are never
  narrower than 1 %.
- Hardware resources: Vary on and Vary off run `VRYCFG` for the
  configuration descriptions over the resource (one command per
  `CFGTYPE`), `*JOBCTL` first; a resource no description uses gets a
  page that says so, with Cancel only. Filters keep a matching row's
  parents. The system unit, I/O processors and adapters (depth 0 to 2)
  carry the `[-]` glyph, as the prototype shows them.
- Time management: Apply runs `CHGSYSVAL` for each changed value and
  `CHGNTPA RMTSYS()` for the servers (`*IOSYSCFG`). OK and Cancel go to
  System status.
- TCP/IP: every change needs `*IOSYSCFG`; Apply runs `CHGTCPDMN` with
  only the parameters changed, `CHGTCPA IPDTGFWD()` for forwarding.
  Interfaces the system makes (the loopback, generated IPv6 addresses)
  have no Remove. The Host table tab is a read-only list; the Domain
  tab is the overview, which holds the domain fields.
- Servers: Start all autostart servers is `STRTCPSVR
  SERVER(*AUTOSTART)` straight from its POST; End and Restart
  (`STRTCPSVR RESTART()`) are confirmed. Properties changes autostart
  with the server's own `CHGxxxA AUTOSTART()`; a server without one shows
  it read-only. `*HTTP` links to `http://<host>:2001/qs400/httpadmin/instances`.
- Performance: the refresh select is `a` with Off, 5, 10, 30 and 60
  seconds (the golden pages'; home keeps `auto`); each display ends the
  user's interval. A job's CPU bar spans 0 to 20 %. Units busy 70 % or
  more are named under Disk activity with the job doing most I/O.

## NetServer

`env::netsvr::NetServer` is NetServer through its APIs, as a program
calling them has it (`netsvrcfg::client`): every list is `QZLSOLST`
(`ZLSL0101` shares, `ZLSL0201` and QS/400's `QSLL0100` attributes) and
every change the API that makes it (`QZLSADFS`, `QZLSCHFS`,
`QZLSADPS`, `QZLSCHPS`, `QZLSRMS`, `QZLSCHSN`, `QZLSCHSG`, `QZLSCHSI`
`ZLSS0100`, `ZLSS0200` and `QSLS0100`, `QZLSSTRS`, `QZLSENDS`,
`QZLSENSS`), run as the user: the API checks the authority (`*IOSYSCFG`,
or owning the directory or output queue shared; `*SECADM` for the
guest; `*OBJMGT` and `*USE` to a profile to enable it), validates and
words the refusal. Navigator checks `*IOSYSCFG` first only where the
API always needs it (attributes, start, end, ending a session); an API
refusal for authority (`CPFB684`, `CPDB680`) is the Not authorized page
with an `AF` entry, any other its message on the page. The sessions are
the service's table (`netsvr` `List`), the disabled users
`netsvr_disabled_users`.

- Routes (in the Network group, tabs Server, File shares, Print
  shares, Sessions, Disabled users): `/netsvr` (status, Start, Start
  and apply pending values, End..., the attributes current and
  pending with Apply), `/netsvr/confirm?op=end`, `/netsvr/shares`,
  `/netsvr/shares/add`, `/netsvr/shares/<name>[/confirm]` (Remove),
  the same under `/netsvr/prtshr`, `/netsvr/sessions`,
  `/netsvr/sessions/<id>[/confirm]` (connections and open files; End),
  `/netsvr/disabled`, `/netsvr/disabled/<user>/confirm` (Enable).
- Apply sends only what changed: `QZLSCHSN` for the names, text and
  allow system name, `QZLSCHSG` for the guest, `ZLSS0100` for the
  rest IBM's format has, `QSLS0100` for allow NetBIOS, the SMB2
  minimum dialect and log sign-on messages. A number that does not
  read goes to the API as a value it refuses, so the refusal is the
  API's (`CPFB686` and its reason); the page keeps what was typed.
- Confirmations show the equivalent `CALL PGM(QSYS/QZLS...)` with an
  8-byte zero error code (an error is then an escape message).
- The fixture (`fixture/netsvr.rs`) is the APIs themselves over
  `netsvrcfg::api::MemHost`: shares `QIBM`, `HOME`, `DOCS` (read only,
  extensions, list `NSUSERS`), print share `PRT01`; three sessions, one
  the guest's; `TESTUSER` disabled; pending idle time-out and dialect
  differing from current.
- The pages have no prototype: their approved renderings are
  `tests/golden/nav/netsvr*.html` (`QS400_UPDATE_GOLDEN=1 cargo test -p
  navigator --test netsvr` rewrites them after a deliberate change);
  `tests/netsvr.rs` drives every action. The prototype's Network pages
  gain the tree's NetServer item (a fix every golden comparison applies).

## NFS

`env::nfs::Nfs` is the export table, the daemon jobs and the mount
table as the storage service keeps them (docs/NFS.md), and `run`, the
command itself as the user: every change is `EXPORTFS`, `STRNFSSVR`,
`ENDNFSSVR`, `MOUNT TYPE(*NFS)` or `UNMOUNT`, so the command checks the
authority, validates and words the refusal. Navigator checks
`*IOSYSCFG` first (each command needs it), with the Not authorized
page and its `AF` entry.

- Network > NFS, three tabs: `/nfs` (Exports: path, access,
  read-write and root hosts, versions, the anonymous user, who
  exported it and when; Add export..., Export all from /etc/exports...),
  `/nfs/add` and `/nfs/exp/<id>` (the options as fields: read only,
  read-write, access and root hosts, anonymous UID, no set-user-ID,
  versions, public file handle, pseudo-root, NFS root, export name,
  four host option rows, and Update /etc/exports for `-F`; the
  command is `EXPORTFS OPTIONS('-I [-F] -O list') DIR() HOSTOPT()`),
  `/nfs/exp/<id>/confirm` (Remove: `EXPORTFS OPTIONS('-U')`, `-U -I
  -F` to take it out of the file too), `/nfs/confirm` (Export all:
  `EXPORTFS OPTIONS('-A')`); `/nfs/servers` (each daemon type in
  `STRNFSSVR SERVER(*ALL)`'s order with its jobs, status and ports;
  `/nfs/servers/<type|ALL>/confirm?op=start|end`); `/nfs/mounts` (the
  mount table; `/nfs/mounts/add` Mount NFS file system,
  `/nfs/mounts/<id>/confirm` Unmount of an NFS mount or a UDFS).
- A refused change stays on its form with the command's message; a
  confirmation shows the equivalent command.
- The fixture (`fixture/nfs.rs`) keeps three exports, an
  `/etc/exports` with one more, the binder, two server jobs and the
  mount daemon, and six mounts, and runs the five commands on them
  (`CPFA1B8` without `*IOSYSCFG`).
- The pages have no prototype: their approved renderings are
  `tests/golden/nav/nfs*.html`; `tests/nfs.rs` drives every action.
- Live (`work/qsjob/src/work/httpnav/nfs.rs`): the mount table is the
  job's IFS client's (`IfsClient::mounts`), as the user; a refusal is
  shown above an empty list.

## The live server

Navigator is licensed program `QS00NAV` `*BASE` (docs/LICPGM.md),
installed on every new system, and HTTP server instance `NAVIGATOR`,
off until started like any other instance: `STRTCPSVR SERVER(*HTTP)
HTTPSVR(NAVIGATOR)` (or Start on HTTP Server Administration's Server
instances) starts job `NAVIGATOR` in `QHTTPSVR` under `QTMHHTTP` (role
`HTTPNAV`, `work/qsjob/src/work/httpnav/`); `ENDTCPSVR ...
HTTPSVR(NAVIGATOR)` ends it. `HTTPSVR(*ADMIN)` no longer starts it;
`HTTPSVR(*ALL)` and `STRTCP` do only when its registry entry is marked
autostart (it is not when made). Without `QS00NAV` the instance does
not exist: not listed, and the command answers `TCP1A06`. It listens
on 2002 and, with TLS, 2005 (the certificate `*SYSTEM` assigns
`QS400_HTTP_SERVER_NAVIGATOR`, else the system's own). The job's thread
holds the link; connection threads hand it closures
(`work/qsjob/src/work/web/`, shared with `ADMIN`). Every read runs with
the job swapped to the signed-on user (the `QSYGETPH` handle taken at
sign-on), so the service checks that user's authority; every change is
`run(user, command)`: the analyzer as the user, the completion or the
escape (the diagnostic before `CPF0006`) the message.

| Area | What it reads |
|---|---|
| System | the name (`SYSNAME`, else `QSYSNAME`), the job's dates, sign-on (`sign_on`, `PW` entries), special authorities and class (`whoami`, `QSYRUSRI`), `web_audit`, message files (`describe`). Status: `panel2_data SYSSTS` (`WRKSYSSTS`: CPU, elapsed, jobs, ASP, temporary storage, main storage, shown as Memory, pools) and `DSKSTS` (units), `machine` (`QSRLNBR`, `QMODEL` and the firmware's vendor and product, `hwrsrc::dmi`), `active_jobs` (active, interactive, `MSGW`, the IPL from `SCPF`'s start), `sbs_list`, the job queues' jobs, `splf_list` (ready files), `msgq_list QSYS/QSYSOPR` |
| Work | `actjob_data` (`WRKACTJOB`), `job_detail` (`DSPJOB`, the library list), `panel2_data JOBLCK` (`DSPJOB OPTION(*JOBLCK)`: the job's locks from the store's lock table, as `WRKOBJLCK`), `list_objects` + `wm_object` (`*JOBQ`, `*SBSD`, `*OUTQ`, `QUSRSYS/QDFTJOBSCD`), `jobq_attach`, `sbs_list`, `pools`, `splf_list`, `splf_fetch` (text and PDF through `spool`), `wtr_list`, `cfg_status` |
| Messages | `list_objects *MSGQ` (`QSYS` `QSYSOPR`/`QSYSMSG`, then `QUSRSYS`), `msgq_list` (`DSPMSG`), the profile's `MSGQ` (`QSYRUSRI`), `hst_entries` (`DSPLOG`) |
| Users | `QSYLAUTU` + `QSYRUSRI` (`sec_api Users`, `Profile`), `QSYLOBJA` (a profile's objects), `autl_panel_list` + `autl_panel` (`WRKAUTL`, `DSPAUTL`), `QSYLATLO` (`DSPAUTLOBJ`). A profile is a group when another names it as `GRPPRF` or `SUPGRPPRF`, or it has a `GID` |
| Status, more | failed sign-ons: `DSPAUDJRNE ENTTYP(PW)`'s report (`exec`), its entries of the last 24 h; PTFs not applied: `sys2_panel PTF` rows `Loaded` (`DSPPTF`); servers down: the registry's servers set to autostart (`tcp Servers`) with no listener among `active_jobs`; machine type: the processor's type in `hw_table` |
| Config | `sysval_panel_list` + `sysval_panel` (`WRKSYSVAL`, `DSPSYSVAL`) with shipped value, allowed values and IPL flag from the `lic/sysval` catalog, the category from its group (`httpnav/config.rs`); `panel2_data DSKSTS` (with `SIZEB`, `SERIAL`, `PROT`, `STATUS` per unit) and `SYSSTS`'s ASP figures; `hw_table` in `WRKHDWRSC *AHW` order (`hwrsrc::view`) and `hw_cfgds` per resource; `QTIMZON`, `QDATFMT`, `QDATSEP`, `QTIMSEP`, `QTIMADJ`, `sysval::tz::ZONES`, `CHGNTPA`'s `RMTSYS` (`tcp Servers`), `QTOTNTP` active |
| Net | `tcp Snapshot` (interfaces, routes), `tcp HostConfig` (domain, host table), `tcp Netstat STS` (active since), `tcp Report attr` (forwarding), the server registry with `tcp Servers` and `active_jobs` |
| NetServer | `netsvr_api` (the `QZLS*` APIs as the user, `work/qsjob/src/work/nsadmin.rs`), `netsvr` `List` (the session table), `netsvr_disabled_users`, `active_jobs` (`QZLSSERVER` in `QSERVER`) |
| NFS | `nfs` `Exports` (the export table, each entry read with `nfs::exports::file`), `nfs` `Daemons` (the daemon registry), `IfsClient::mounts` (the mount table); every change the command through the analyzer as the user (`Link::run`) |
| Perf | `panel2_data SYSACT` (`WRKSYSACT`), `SYSSTS RESET(*YES)`'s pools, `DSKSTS RESET(*YES)` |
| Db | queries over the `QSYS2` catalog views through the interactive SQL session (`isql_start` `*SQL`/`*NONE`, `isql_run`): `SYSSCHEMAS`, `SYSTABLES`, `SYSTABLESTAT`, `SYSCOLUMNS`, `SYSCST` + `SYSKEYCST`, `SYSINDEXES` + `SYSKEYS`, `SYSVIEWDEP`; creation times from `object`. Run SQL: `isql_start` with the naming and isolation, `SET SCHEMA`, `isql_run` (the row cap), `isql_end` commits; messages from `QSQLMSG` as Enter SQL Statements words them. No `SYSTRIGGERS`/`SYSROUTINES` (no triggers, procedures), no `QSQGNDDL` (Generate SQL answers `SQL0084`), no Explain (`SQL0084`) |
| Security | `DSPAUDJRNE ... OUTPUT(*)` through `exec`, as its 5250 display runs it (whole UTC days, cut to the filter's seconds), `secaud_panel` (`DSPSECAUD`), the receiver's creation (`object *JRNRCV`) as its attach time |
| Software | `licpgm_panel` `INSTALLED`, `SFWRSC` (libraries), `LICINF` (usage limits), `MEDIA` of a catalog's device; `panel2_data IMGCLG` (`WRKIMGCLG`); `sys2_panel` `LICKEY`, `PTF` per installed product, `PTFD` (delayed), `PTFGRP`, `PTFCVR` (cover letters; a PTF's text is its letter's first line); data area `QSYS/QSUPDATTR` (`api_dtaara_read`). Fixes are the lines of the last `QSCHKUPD` run from Navigator (newer groups, missing HIPER PTFs, a cumulative package), kept in memory |

Not live yet: another job's job log and call stack (the 5250 displays
refuse them too, `CPD9B9D`), so those tabs are empty; Time's last
synchronization (the SNTP client keeps none); per-user performance
intervals (the service keeps one per display, so a refresh here also
restarts `WRKSYSACT`'s); disk service and wait times and a unit's top
job; when a PTF was loaded and a product installed (neither display
shows it). Active jobs' notes: a job a user ended through Navigator
(`ENDJOB` that completed) is reported to that user once, when it has
left the active job table, as the job's `CPF1164` from the history
log (the golden page shows `CPF1321`, the prototype's choice). QEMU step 88
(`tools/conformance/src/navstep.rs`) starts `HTTPSVR(NAVIGATOR)`, signs on over 2002, checks System
status against `DSPNETA`, the `NAVIGATOR` job on Active jobs against
`DSPJOB`, `QSYSOPR`, `QSECURITY` on Security values against
`DSPSYSVAL`, `10.0.2.15` on interfaces, `*HTTP` started on TCP servers,
System activity, the CSRF refusal, Run SQL's result table over
`QSYS2.SYSTABLES`, `QS00SS1` and `QS00NAV` on Licensed programs, PTFs, the audit
journal and sign-off, and the sign-on page over TLS on 2005.
## Database, security, software

`env::db::Db` reads the catalog views (`QSYS2.SYSSCHEMAS`, `SYSTABLES`,
`SYSTABLESTAT`, `SYSCOLUMNS`, `SYSCST`, `SYSINDEXES`, `SYSTRIGGERS`)
and runs one statement as the user (`run_sql`, the interactive SQL
session's `isql_run` with its naming, isolation and row limit);
`env::sec::Security` is `DSPAUDJRNE` and `DSPSECAUD`;
`env::sw::Software` is `GO LICPGM`'s data (`licpgm_panel`), the image
catalogs, `DSPLICKEY`, `DSPPTF`, `WRKPTFGRP`, `DSPPTFCVR` and the update
attributes (`QSYS/QSUPDATTR`), and runs every change through
`Software::run` as the user.

- Routes: `/sql[?q=&s=|?h=N|?sort=|?csv=1]`, `/db[/<schema>][?show=
  tables|views|indexes|procedures&sort=t|r]`, `/db/<s>/<t>/{columns,
  keys,indexes,triggers,properties,ddl}`, `/db/<s>/<t>/data?w=&sort=&
  page=[&csv=1]`, `/audit?t=&u=&f=&to=&page=[&csv=1]`, `/licpgm?s=&c=`,
  `/licpgm/keys`, `/licpgm/<id>/<option>[/confirm?op=delete|install&c=]`,
  `/ptf?p=&s=&id=&sort=&page=[&check=]`, `/ptf/confirm?op=load|apply|
  remove|order&id=...[&c=&t=]`, `/ptf/level`, `/ptf/<id>/cover`,
  `/fixes`, `/fixes/<id>/cover`.
- Run SQL keeps, per user and in memory only (`Navigator::sql`), the
  last 50 statements (newest first, each once) and the last statement's
  result: the POST runs (or explains) and redirects, the GET shows. A
  query shows at most 500 rows; CSV runs the query again for every row.
  Its message line is `SQL7958` (rows, seconds) and `SQL0100` at the end
  of the data, then the statement's own messages. Previous statements
  leave out the one shown.
- View data is Run SQL's result table over `SELECT * FROM S.T [WHERE w]
  ORDER BY <primary key or the sorted column>`, 25 a page, with Edit in
  Run SQL carrying the statement (`/sql?q=`).
- The audit journal needs `*AUDIT`; the dates are the user's days, the
  page is `DSPAUDJRNE`'s print (time, type, user, job, detail) in a
  `<pre>`. The home page's `/audit?t=PW` selects `PW - Password`.
- Licensed programs: Installed lists the product directory, which
  includes products it knows but has not installed (their Install uses
  the image catalog chosen); the catalog view shows the directory's state
  of each product on it. Install is `RSTLICPGM ... DEV(<the catalog's
  virtual device>)`, Delete `DLTLICPGM`, both confirmed, both `*ALLOBJ`
  first, as every software change (`LODPTF`, `INSPTF`, `APYPTF`,
  `RMVPTF`, `SNDPTFORD`, `QSCHGUPDA`). Check compatibility is
  `CHKPRDOPT PRDID(*ALL)` straight from its POST.
- PTFs: Load from image catalog is `INSPTF LICPGM(*ALL) DEV(<device>)
  INSTYPE(*DLYALL)`; a PTF's Load is `LODPTF DEV(*SAVF) SAVF(QGPL/<id>)`
  (where `SNDPTFORD` leaves it); Apply asks *TEMP/*PERM and whether the
  delayed ones wait for the next IPL (`DELAYED(*YES)`) or the next
  unattended IPL (`IPLAPY(*YES)`), one `APYPTF` per product. Check for
  fixes (`QSCHKUPD`) runs from the filter's GET button: it reads the
  update service and changes nothing but the time of the last check.
  `CPI3599` under the filter counts the fixes the last check found
  missing. Cover letters open in the help window's layout.
- Fixes: Download is `SNDPTFORD PTFID((<id>))` (`*CUMPKG` for a
  cumulative package), confirmed. The settings run `QSCHGUPDA`; the
  Server field is the URL's host (a host alone becomes
  `https://<host>/qs400`), an empty proxy `*NONE`.

## Golden comparison

`tests/common` drives the handler with a browser in memory over the
fixture; `proto()` maps every served URL to its prototype file.
`assert_golden(served, file, fixes)` drops the CSRF field, ignores
whitespace between tags and compares the rest. Fixes are real defects
of the golden page:

| Page | Fix | Why |
|---|---|---|
| `nav/signon.html` | form `action` `home.html` → `signon.html` | the form posts to the sign-on route; home follows by redirect |
| `nav/job-props-liblist.html` | Add library..., Remove, Change current library... `disabled` | only the job itself changes its library list (`CHGLIBL`); Navigator runs in another job |
| `nav/job-props-joblog.html` | `7 messages.` → `6 messages.` | the page lists six |
| `nav/job-props-callstack.html` | the Thread select and Go wrapped in `<form method="get">` | they sat outside any form, so Go did nothing |
| `nav/subsystems.html`, `nav/printers.html`, `nav/scheduled-jobs.html` | the two buttons get names `start`/`add` and `refresh` | two nameless buttons in one form cannot be told apart |
| `nav/subsystem-props-pools.html` | Add pool..., Change..., Remove get names `add`, `change`, `remove` | the same |
| `nav/qsysopr.html` | rows 11-25 not compared; `123499/QPGMR/RGZPFM00`, `123150/QSPLJOB/PRT01`, `123380/QTCP/QTCPIP` not links | the prototype repeats its first ten messages to fill the page (the fixture's 41 are distinct, three inquiries waiting); those jobs are not in the system (ended, or another job has the number) and a link would lead nowhere |
| `nav/message-detail.html` | the Reply field starts empty; its button is named `send` | a prefilled reply other than the default goes out on one click; the field and the button were both named `reply` |
| `nav/history-log.html` | message IDs linked; six night entries added to the page | message IDs are links everywhere (handoff rule 3); the page said 25 rows of 214 but showed 19 |
| `nav/users.html` | the two buttons get names `create`/`refresh` | two nameless buttons in one form cannot be told apart |
| `nav/user-props.html` | Group profile offers every group; Reset password... named `reset` | the select left out three of the six groups; the button shared the Apply form without a name |
| `nav/spooled-view.html` | `< Previous` on page 1 grey text; Next, View as PDF, Download text links `#` → the view's routes; the Hold, Release, Delete buttons → links like the list's | the links went nowhere; the buttons sat in the page's GET form, so Delete would have run by a bare GET without its confirmation |
| `nav/tcpip.html` | `13 started` → `14 started` | the Servers page lists fourteen started |
| `nav/tcpip-interfaces.html`, `nav/servers.html` | the two buttons get names `add`/`start` and `refresh` | two nameless buttons in one form cannot be told apart |
| `nav/tcpip.html`, `nav/tcpip-interfaces.html`, `nav/tcpip-routes.html`, `nav/servers.html` | the tree's Network group gains NetServer and NFS after Servers (`tests/common` applies it to every page) | NetServer and NFS came after the prototype |
| `nav/run-sql.html` | `SQL0100`'s text → the message file's (`No row was found for FETCH.`); Download as CSV `#` → `?csv=1` | the prototype quoted another vendor's message text; the link went nowhere |
| `nav/table-columns.html` | Generate SQL (DDL) `#` → `/ddl` | the link went nowhere |
| `nav/table-data.html` | the list is the table's first 25 rows, all 12 columns; `12 rows returned` → `412 rows returned`; CSV `#` → `?csv=1` | the prototype pasted Run SQL's 8-column, 12-row result under `SELECT *` of a 412-row table paged 25 at a time |
| `nav/audit-journal.html` | eleven more entries on the page; CSV `#` → `?csv=1` | the page said 25 rows of 762 but showed 14 |
| `nav/home.html` | `Model / type` → `Model`, the hardware's vendor and product (`Dell Inc. PowerEdge R650`; `Not reported` when the firmware names none); `Main storage` → `Memory` | Cody (2026-11-02): `QMODEL` and `QPRCFEAT` stay read-only system values for programs, but no display shows them; his wording for the rows |
| `nav/licensed-programs.html`, `nav/ptfs.html`, `nav/ptf-confirm.html`, `nav/fixes.html` | product IDs `5Q00xxx` → `QS00xxx` | the system's products are `QS00SS1`, `QS00WDS`, ... (`licpgm/licpgm/catalog.toml`) |
| `nav/licensed-programs.html`, `nav/ptfs.html` | the three buttons get names `install`, `check`, `keys` and `load`, `apply`, `level` | three nameless buttons in one form cannot be told apart |
| `nav/ptf-confirm.html` | `QS10447`'s description → the list's (`Kernel: IPL time with large ASP`); the options moved into the form | the two pages disagreed; the radio buttons sat outside the form, so OK never sent them |

## Steps

The handoff's build order, one commit each; `tests/behavior.rs`'s
`PENDING` names the routes not built yet and each step removes its own.

| Steps | Area | State |
|---|---|---|
| 1, 2 | Frame, sign-on, System status | done |
| 3-11 | Work management | done (golden-clean; live) |
| 12-14 | Messages | done (golden-clean; live) |
| 15-18 | Users and groups | done (golden-clean; live) |
| 19-22 | System | done (golden-clean; live `Config` pending) |
| 23, 24 | Network | done (golden-clean; live `Net` pending) |
| 25, 26 | Database | done (golden-clean; live `Db` pending) |
| 27-29 | Security, software | done (golden-clean; live `Security`, `Software` pending) |
| 30 | Performance | done (golden-clean; live `Perf` pending) |
| 31 | NetServer | done (approved pages; live) |
| 32 | NFS | done (approved pages; live) |
