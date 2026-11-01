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
| `nav/spooled-view.html` | `< Previous` on page 1 grey text; Next, View as PDF, Download text links `#` → the view's routes; the Hold, Release, Delete buttons → links like the list's | the links went nowhere; the buttons sat in the page's GET form, so Delete would have run by a bare GET without its confirmation |

## Steps

The handoff's build order, one commit each; `tests/behavior.rs`'s
`PENDING` names the routes not built yet and each step removes its own.

| Steps | Area | State |
|---|---|---|
| 1, 2 | Frame, sign-on, System status | done |
| 3-11 | Work management | done (golden-clean; live `Work` pending) |
| 12-14 | Messages | done (golden-clean; live `Msgs` pending) |
| 15-18 | Users and groups | |
| 19-22 | System | |
| 23, 24 | Network | |
| 25, 26 | Database | |
| 27-30 | Security, software, performance | |
