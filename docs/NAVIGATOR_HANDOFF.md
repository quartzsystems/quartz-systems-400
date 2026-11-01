# Implementation handoff: Navigator for QS/400 (ports 2002 / 2005)

For Claude Code. This zip is the approved design for one product: Navigator for QS/400, the system management web interface. It runs as HTTP server instance `NAVIGATOR` in subsystem `QHTTPSVR` on port 2002 (HTTP) and 2005 (HTTPS, certificate application `QS400_HTTP_SERVER_NAVIGATOR`). Make a real server produce these pages from live system state.

## What is in the zip

```
qs400-navigator/
  index.html              redirects to nav/signon.html
  shared/qs400.css        the ONLY stylesheet. Ship it byte-for-byte.
  shared/components.html  every pattern once: frame, message area, list, properties, wizard, controls
  shared/confirm.html  shared/error.html  shared/help.html  shared/notauth.html   the generic pages, framed for this product
  nav/                    55 pages of this product (golden renderings)
  STYLE.md                colours, type, table rules, verbs, message rules, what-not-to-do
  HANDOFF.md              this file
```

Pages of other products that this product links to directly are included so the prototype clicks through; they are reference only and are implemented by their own product:

- `http/instances.html` (from HTTP Server Administration)

## Ground rules (do not negotiate these away)

1. **Server-rendered HTML, full-page reloads.** Forms are `method="post"`; lists sort and page with `?sort=` / `?page=` / `?rows=` query strings; confirmation is a separate GET page that POSTs back. The only JavaScript permitted anywhere in the QS/400 web products is the 5250 session page of Access for Web (WebSocket terminal). Every other page must work with script disabled, and must remain usable with CSS disabled.
2. **No new CSS.** `shared/qs400.css` (177 lines) ships byte-for-byte. If a page seems to need a class the stylesheet lacks, build it from tables and text the way the prototype does. If it is truly unavoidable, append under an `/* addition */` comment and record it in `STYLE.md`.
3. **Message IDs are links everywhere** (`CPF1321`, `HTP8001`, `SQL0100`). The link target is the second-level text page for that ID (`shared/error.html` shows the layout: Message, Cause, Recovery, in that order).
4. **System output is Courier and column-exact**: job logs, spooled files, SQL results, configuration files, command strings. Never reflow it.
5. **Verbs match the commands behind them**: Start/Stop/Restart for HTTP instances, Hold/Release/End for jobs, Start/End for writers, servers and subsystems, Vary on/off for devices, Load/Apply/Remove for PTFs. Dangerous actions always go through the confirmation page; none is reachable by a bare GET.
6. **No IBM product names, feature names, wording or marks** anywhere in code, templates, messages, docs or tests. Names in use: QS/400, Quartz Systems, Navigator for QS/400, QS/400 Access for Web, QS/400 HTTP Server Administration. Message prefixes: CPF/CPA/CPI/CPC (system), SQL (database), HTP (HTTP server), TCP (network).
7. **Commits**: per section, under Cody's git identity only (never as yourself); one-line conventional messages. Never push.

## Shared infrastructure

The three QS/400 web products share one crate (suggested name `qsweb`; read the workspace layout first and put it where the other licensed-program crates live). Whichever product is built first builds the crate; the others reuse it unchanged. If it already exists in the repository when you start, use it and do not fork it.

**HTTP server.** Each product is one HTTP server instance running as a job in subsystem `QHTTPSVR` under user profile `QTMHHTTP` (`ADMIN` on 2001, `NAVIGATOR` on 2002/2005, `ACCESSWEB` on 2003/2006). Keep the HTTP layer small: HTTP/1.1, keep-alive, TLS from the certificate manager's `*SYSTEM` store through the product's certificate application ID, multipart upload, cookies. No framework that drags in a JavaScript build step.

**Templating.** Render with a compile-time template crate (askama-style) so the DOM is identical on every request. One base template = the frame in `shared/components.html`:

```
header band  (wordmark | System: QSHQ400  User: <user>  Sign off  Help)
nav column   (tree: [+]/[-] glyph links, current group open, current item bold)
content      (breadcrumb, h1, grey description, message area, body)
footer       (QS/400 V1R0M0 · Quartz Systems)
```

and five partials: `message_row(kind, msgid, text)`, `list_table(cols, rows, sort, empty_text)`, `paging(page, of, rows, total)`, `props(groups, buttons)`, `wizard(steps, current, body, buttons)`. The prototype was generated from exactly these five helpers, so once your partials reproduce `shared/components.html` the product pages follow.

**Sessions and sign-on.** The product's sign-on page: User + Password only. Authenticate against the QS/400 user profile with the 5250 sign-on rules (`QMAXSIGN`, disabled profiles, expired passwords → message area, failed attempts → audit journal PW entries). Session = opaque cookie, server-side table, idle timeout from product settings. "Sign off" on every page invalidates it and returns to the product's own sign-on page. CSRF: every POST form carries a hidden per-session token; reject POSTs without it. POST → redirect → GET, the completion message carried in the session so it shows once in the next page's message area.

**Authority.** Every action performs the authority check of the equivalent command (`*IOSYSCFG`, `*JOBCTL`, `*SECADM`, `*ALLOBJ`, object authority). A failure renders `shared/notauth.html` with the real message (`CPF2207`-style) and writes the audit journal AF entry.

**Help.** Every page has a help key; `Help` in the header and the small `?` links open `/help/<key>` in a new window using the `shared/help.html` layout. Second-level message text is served at `/msg/<MSGID>` with the `shared/error.html` layout.

**Conventions.** URLs mirror the prototype file names (`<product>/instance-ports.html` → `GET …/instance/<name>/ports`). Sort parameter values are the column keys already in the prototype's `?sort=` links. Default 25 rows per page; the rows select offers 10/25/50/100. Dates `MM/DD/YY`, times `HH:MM:SS`, as the user's job attributes say.

## How to use the golden pages

Every HTML file in this zip is a golden rendering: the same page served live with the same data must produce the same DOM (whitespace and the CSRF token aside). Each was produced by frame(product, nav, breadcrumb, title, description, message?) + a body built from the five partials. Port the partials first, render `shared/components.html` from them, and only then start on the product pages. When a live page differs from its golden file, the golden file wins unless the difference is data.

Acceptance for every step: the page diffs clean against its golden file with the fixture data; every link and form action on the served page resolves (keep a crawler test); script disabled and CSS disabled the page is still operable; list pages show 25 rows at 1024×768 without scrolling the page body; no IBM string anywhere in the crate (grep in CI).

Do not add: modals, spinners, toasts, inline editing, icons beyond text glyphs, flexbox/grid, web fonts, a JS bundle, or a second stylesheet.


## Product scope

Navigator is a web face over existing system facilities; it adds no state of its own beyond the user's session and the Run SQL history. Every page is the HTML form of a command or API that already exists (named in the table). Use those interfaces, never a private path into the object store, so that what Navigator shows is exactly what the 5250 command shows.

Navigation tree: System (System status, System values, Disk status, Hardware resources, Time management); Work management (Active jobs, Job queues, Subsystems, Output queues, Printers, Scheduled jobs); Messages (QSYSOPR, My messages, History log); Users and groups (Users, Groups, Authorization lists); Network (TCP/IP configuration, Interfaces, Routes, Servers); Database (Schemas, Run SQL); Security (Audit journal, Security values); Software (Licensed programs, PTFs, Fixes); Performance (System activity, Disk activity). Route prefix `/qs400/navigator/`.

## Build order

One step = one commit, ending with the page matching its golden file. Steps are grouped so that each group is independently shippable.

### Group 1 — frame, sign-on, home

| Step | Golden page(s) | Route(s) | Backend |
|---|---|---|---|
| 1 | `nav/signon.html` | `GET/POST /signon` | user profile auth, session |
| 2 | `nav/home.html` | `GET /home?auto=60` | system status API (CPU %, jobs in system, active, ASP %, main storage, IPL time, serial, model); `<meta http-equiv="refresh">` from the `auto` select; every value links to its detail page |

### Group 2 — work management (the pages operators live on)

| Step | Golden page(s) | Route(s) | Backend |
|---|---|---|---|
| 3 | `nav/active-jobs.html` (+ `-empty`, `-error`) | `GET /jobs?sbs=&user=&job=&sts=&sort=&page=` | `WRKACTJOB` data: qualified job name, type, subsystem, status, CPU % over the elapsed interval, function, current user; elapsed interval reset on Refresh |
| 4 | `nav/end-job-confirm.html` | `GET /job/<qual>/end` → `POST` | `ENDJOB OPTION(*CNTRLD\|*IMMED) DELAY(n) SPLFILE(*NO\|*YES) LOGLMT ADLINTJOBS`; the page shows the equivalent command string |
| 5 | Hold / Release row actions | `POST /job/<qual>/hold`, `/release` | `HLDJOB` / `RLSJOB`; `CPF1317`-style failures to the message area (`nav/active-jobs-error.html`) |
| 6 | `nav/job-props-general.html`, `-performance`, `-liblist`, `-joblog`, `-locks`, `-callstack` | `/job/<qual>/{general,performance,liblist,joblog,locks,callstack}` | `DSPJOB` options; job log is a Courier `<pre>` with message IDs linked, severity filter and last-N select; call stack per thread; General tab Apply = `CHGJOB` for run priority, time slice, logging, inquiry reply |
| 7 | `nav/job-queues.html` | `/jobq?lib=&s=` (+ hold/release/clear POSTs) | `WRKJOBQ`, `HLDJOBQ`, `RLSJOBQ`, `CLRJOBQ` (clear via confirmation) |
| 8 | `nav/subsystems.html`, `nav/subsystem-props.html`, `-pools`, `-routing`, `-autostart` | `/sbs`, `/sbs/<name>/{general,pools,routing,autostart,workstation,prestart}` | `WRKSBS`, `DSPSBSD`, `STRSBS` / `ENDSBS` (end via confirmation), `CHGSBSD`, `ADDRTGE`/`CHGRTGE`/`RMVRTGE`, `ADDAJE`/`RMVAJE` |
| 9 | `nav/output-queues.html`, `nav/spooled-files.html`, `nav/spooled-view.html` | `/outq`, `/outq/<lib>/<name>?user=&ud=&s=&ft=&page=`, `/splf/<job>/<file>/<nbr>?p=&f=` | `WRKOUTQ`, `WRKSPLF`, `HLDSPLF`/`RLSSPLF`/`DLTSPLF`/`CHGSPLF OUTQ()`; view renders the spooled file as 132-column text with page navigation and Find, plus a PDF rendering link |
| 10 | `nav/printers.html` | `/writers` (+ start/end writer, vary on/off POSTs) | `WRKWTR`, `STRPRTWTR`, `ENDWTR` (confirmation), `VRYCFG` |
| 11 | `nav/scheduled-jobs.html` | `/jobscde` (+ add/change/hold/release/remove/submit-now) | `WRKJOBSCDE`, `ADDJOBSCDE`, `CHGJOBSCDE`, `HLDJOBSCDE`, `RLSJOBSCDE`, `RMVJOBSCDE`; command column in Courier |

### Group 3 — messages

| Step | Golden page(s) | Route(s) | Backend |
|---|---|---|---|
| 12 | `nav/qsysopr.html`, `nav/my-messages.html` | `/msgq/<lib>/<name>?t=&d=&page=` | `DSPMSG`; inquiry messages awaiting reply rendered bold; Remove; Send message form (`SNDMSG` / `SNDMSG MSGTYPE(*INQ)`) |
| 13 | `nav/message-detail.html` | `/msgq/<lib>/<name>/<key>` (+ Reply POST) | second-level text (Message, Cause, Recovery), from job and program, valid replies and default; `SNDRPY`; reply refused if already answered |
| 14 | `nav/history-log.html` | `/qhst?f=&ft=&t=&tt=&id=&sev=&job=&page=` | `DSPLOG`, newest first, Courier, message IDs linked |

### Group 4 — users and groups

| Step | Golden page(s) | Route(s) | Backend |
|---|---|---|---|
| 15 | `nav/users.html`, `nav/user-props.html` | `/users?p=&s=&c=&g=`, `/user/<name>/{general,groups,authorities,jobs,personal}` | `WRKUSRPRF`, `DSPUSRPRF`, `CHGUSRPRF` (Apply), Reset password (`CHGUSRPRF PASSWORD() PWDEXP(*YES)`), Delete via confirmation (`DLTUSRPRF` with owned-object handling asked on the confirmation page) |
| 16 | `nav/create-user-1.html`, `nav/create-user-5.html` (steps 2–4 follow the same wizard frame) | `/users/create/step/1..5` | `CRTUSRPRF`; the summary step shows the exact command string; optional home directory and message queue creation |
| 17 | `nav/groups.html` | `/groups` | group profiles (profiles with a GID / members), Members list, Create group |
| 18 | `nav/authorization-lists.html`, `nav/autl-entries.html` | `/autl`, `/autl/<name>` (+ add/change/remove) | `WRKAUTL`, `DSPAUTL`, `ADDAUTLE`/`CHGAUTLE`/`RMVAUTLE`, `DSPAUTLOBJ` for the Objects link |

### Group 5 — system

| Step | Golden page(s) | Route(s) | Backend |
|---|---|---|---|
| 19 | `nav/system-values.html`, `nav/security-values.html`, `nav/sysval-change.html` | `/sysval?cat=security`, `/sysval/<name>` | `WRKSYSVAL` by category; Change = `CHGSYSVAL` with validation of the allowed values shown in grey; audit SV entry |
| 20 | `nav/disk-status.html` | `/disk` | ASP summary and `WRKDSKSTS` units, utilisation bars from `table.bar`, Reset clears the I/O counters |
| 21 | `nav/hardware-resources.html` | `/hdwrsc?t=&s=` | `WRKHDWRSC` tree as an indented table; Vary on / Vary off via `VRYCFG` and the confirmation page |
| 22 | `nav/time-management.html` | `/time` | `QTIMZON`, `QDATFMT`, `QTIMADJ`, NTP client status |

### Group 6 — network

| Step | Golden page(s) | Route(s) | Backend |
|---|---|---|---|
| 23 | `nav/tcpip.html`, `nav/tcpip-interfaces.html`, `nav/tcpip-routes.html` | `/tcp`, `/tcp/ifc`, `/tcp/rte` | `CFGTCP` options: `CHGTCPDMN`, `ADDTCPIFC`/`CHGTCPIFC`/`RMVTCPIFC`/`STRTCPIFC`/`ENDTCPIFC`, `ADDTCPRTE`/`RMVTCPRTE`; `*IOSYSCFG` required to change |
| 24 | `nav/servers.html` | `/tcp/servers` (+ start/end/restart POSTs) | `STRTCPSVR` / `ENDTCPSVR` per server, autostart attribute (`CHGxxxA AUTOSTART()`), ports; the `*HTTP` row links to the HTTP Server Administration product on port 2001 |

### Group 7 — database

| Step | Golden page(s) | Route(s) | Backend |
|---|---|---|---|
| 25 | `nav/run-sql.html` | `GET/POST /sql` | run one statement under the user's authority with the chosen schema, naming and isolation; result capped at 500 rows with a CSV download of the full result; message line with `SQL7958` / `SQL0100`-style completion text; previous statements kept per user (last 50) |
| 26 | `nav/schemas.html`, `nav/table-columns.html`, `nav/table-data.html` | `/db`, `/db/<schema>`, `/db/<schema>/<table>/{columns,keys,indexes,triggers,properties}`, `/db/<schema>/<table>/data?w=&page=` | catalog views; View data reuses the Run SQL result table with a Where filter |

### Group 8 — security, software, performance

| Step | Golden page(s) | Route(s) | Backend |
|---|---|---|---|
| 27 | `nav/audit-journal.html` | `/audit?t=&u=&f=&to=&page=` | `DSPAUDJRNE` over `QSYS/QAUDJRN`, Courier, CSV download |
| 28 | `nav/licensed-programs.html` | `/licpgm?s=&c=` | `GO LICPGM` options 10/11/12: installed products, install from image catalog (confirmation), delete (confirmation), license keys |
| 29 | `nav/ptfs.html`, `nav/ptf-confirm.html`, `nav/fixes.html` | `/ptf?p=&s=&id=&page=`, `/ptf/confirm?op=load\|apply\|remove`, `/fixes` | `DSPPTF`, `LODPTF`, `APYPTF` (temp/perm, delayed), `RMVPTF`, cover letters; "Check for fixes" against `update.quartz.systems` with the configured schedule |
| 30 | `nav/system-activity.html`, `nav/disk-activity.html` | `/perf/sys?auto=10`, `/perf/disk?auto=10` | `WRKSYSACT` / `WRKSYSSTS` pools, `WRKDSKSTS` I/O rates; `<meta refresh>`; the only "chart" is `table.bar` |

Messages this product issues come from the underlying commands; Navigator adds none of its own except `CPI9801`-style informational rows for its own paging and filter conditions, which may reuse system messages.

## Fixture data

Load the prototype's sample data as fixtures: system `QSHQ400`, 318 active jobs across `QINTER`/`QBATCH`/`QHTTPSVR`/`QSYSWRK`/`QUSRWRK` with two in `MSGW`, output queue `QGPL/QPRINT` with 148 spooled files, 41 QSYSOPR messages with three unanswered inquiries, fifteen user profiles, 118 PTFs at cumulative level `QS400-CUM-26290`, schema `HLINV` with the `ASSETS` table.

## Product-specific acceptance

- Every list page shows 25 rows at 1024×768 without scrolling the page body (`active-jobs`, `spooled-files`, `qsysopr`, `ptfs` are the reference pages).
- `spooled-view` preserves 132 columns exactly; a diff of the served text against the spooled file's own text is empty.
- An inquiry reply from `message-detail` or `qsysopr` round-trips to the real message queue and clears the bold row.
- End job, End subsystem, Clear job queue, Delete user, Delete spooled file, Apply/Remove PTF, Vary off are reachable only through the confirmation page and a POST with the CSRF token.
- Auto-refresh pages carry `<meta http-equiv="refresh">` only when the select is not Off.
