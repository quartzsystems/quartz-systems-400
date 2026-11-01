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
| `nav/spooled-view.html` | `< Previous` on page 1 grey text; Next, View as PDF, Download text links `#` → the view's routes; the Hold, Release, Delete buttons → links like the list's | the links went nowhere; the buttons sat in the page's GET form, so Delete would have run by a bare GET without its confirmation |
| `nav/tcpip.html` | `13 started` → `14 started` | the Servers page lists fourteen started |
| `nav/tcpip-interfaces.html`, `nav/servers.html` | the two buttons get names `add`/`start` and `refresh` | two nameless buttons in one form cannot be told apart |

## Steps

The handoff's build order, one commit each; `tests/behavior.rs`'s
`PENDING` names the routes not built yet and each step removes its own.

| Steps | Area | State |
|---|---|---|
| 1, 2 | Frame, sign-on, System status | done |
| 3-11 | Work management | done (golden-clean; live `Work` pending) |
| 12-14 | Messages | |
| 15-18 | Users and groups | |
| 19-22 | System | done (golden-clean; live `Config` pending) |
| 23, 24 | Network | done (golden-clean; live `Net` pending) |
| 25, 26 | Database | |
| 27-29 | Security, software | |
| 30 | Performance | done (golden-clean; live `Perf` pending) |
