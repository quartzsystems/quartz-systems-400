# Implementation handoff: QS/400 Access for Web (ports 2003 / 2006)

For Claude Code. This zip is the approved design for one product: QS/400 Access for Web, the end-user web interface (5250 session in the browser, database, files, print, messages, jobs, commands, preferences, policies). It runs as HTTP server instance `ACCESSWEB` in subsystem `QHTTPSVR` on port 2003 (HTTP) and 2006 (HTTPS, certificate application `QS400_HTTP_SERVER_ACCESSWEB`). Make a real server produce these pages from live system state.

## What is in the zip

```
qs400-access-for-web/
  index.html              redirects to web/signon.html
  shared/qs400.css        the ONLY stylesheet. Ship it byte-for-byte.
  shared/components.html  every pattern once: frame, message area, list, properties, wizard, controls
  shared/confirm.html  shared/error.html  shared/help.html  shared/notauth.html   the generic pages, framed for this product
  web/                    40 pages of this product (golden renderings)
  STYLE.md                colours, type, table rules, verbs, message rules, what-not-to-do
  HANDOFF.md              this file
```

Pages of other products that this product links to directly are included so the prototype clicks through; they are reference only and are implemented by their own product (Access for Web reuses Navigator's job log, job properties, spooled file viewer, message detail and End Job pages by linking to them on port 2002, or embeds the same templates if Navigator is not installed):

- `http/instance-general.html` (from HTTP Server Administration)
- `nav/active-jobs.html` (from Navigator)
- `nav/end-job-confirm.html` (from Navigator)
- `nav/job-props-general.html` (from Navigator)
- `nav/job-props-joblog.html` (from Navigator)
- `nav/message-detail.html` (from Navigator)
- `nav/spooled-view.html` (from Navigator)
- `nav/table-columns.html` (from Navigator)

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

Access for Web is the one product whose users are not operators: a policy layer (`web/policies.html`) gates every function per user profile and group, **in addition to** normal object authority, and is checked user → groups → `*PUBLIC` with Deny winning. Everything the user does runs under the user's own profile through the host servers (database, remote command, file, sign-on) and the Telnet server for 5250. Nothing runs under `QTMHHTTP`.

The 5250 session page is the single page in all QS/400 web products that carries JavaScript. The server side is a telnet-5250 client to the local Telnet server (port 23, or 992 with TLS); the browser side draws the 24×80 or 27×132 grid from a WebSocket stream and sends keys back. The function-key row below the grid also posts as an ordinary form, so the page remains usable without script (the server then renders the last screen as text).

Navigation tree: My home page; 5250 session; Database (Tables, Run SQL, My requests, Copy data to table); Files (Browse files, My files); Print (Printer output, Printers); Messages; Jobs (My jobs, Server jobs); Commands (Run command, My commands); Customize (Preferences, Policies); Download. Route prefix `/qs400/accessweb/`.

## Parity with QS/400 Access for Desktop

Reviewed against `quartz-systems-access` 1.1.0 (V1R0M0, QAP versions 1–6). Access for Web is the same product in a browser: same session files, same transfer requests, same 5250 grid. Where the desktop reaches QS/400 over QAP (port 2320, TLS 1.3, `QS400_ACCESS_SERVER`), the web server does the same on the user's behalf from the `ACCESSWEB` instance, one QAP session per signed-on user, with a connection of its own per Run SQL / Run Command / transfer page (at most 16 per session, `QAD7003`).

### The 5250 display — pixel-identical to the desktop and the service console

| | Rule (from `qsa-ui/src/console.rs`, `assets/console/SOURCE.md`) |
|---|---|
| Font | the console's 9×16 bitmap font `qs400-9x16.txt` (`lic/console/font`, vendored in both repositories; ASCII 0x20–0x7E, eleven box-drawing glyphs, U+E000 column separator; anything else draws the hollow box) |
| Grid | 24×80 → 720×400 px, 27×132 → 1188×448 px at 1×, the OIA row included (rows + 1) |
| Scale | whole numbers only, nearest neighbour, the grid centred on the background; never smoothed or stretched. The web page offers 1× and 2× and never a fractional CSS size |
| Palette | background `000000`, green `33CC33`, white `E6E6E6`, red `E03030`, blue `5080FF`, turquoise `20C8C8`, pink `E060C0`; attribute bytes `0x20–0x27` green (white when bit `0x02`), `0x28–0x2F` red, `0x30–0x37` turquoise with column separators, `0x38–0x3F` pink (blue when bit `0x02`); `0x01` reverse, `0x04` underline, `x7`/`xF` nondisplay; an attribute byte's own cell is blank |
| Cell drawing | underline lights scanline 14; column separators light the cell's first and last column on scanlines 0, 4, 8, 12; the cursor is a steady white scanline 15 on top of everything; blink is not drawn |
| Black on white | background `E6E6E6`, green and white text `000000`, other colours unchanged, cursor black |
| Disconnected | the last screen stays, every colour averaged with the background (`Look.greyed`) |
| OIA (row below the grid) | col 1 `X SYSTEM` / `X II` / `X II nnnn` in white; col 12 `MW`; col 16 `INS`; col 21 `TLS`; col 26 system name and state; cols−5 `rr/ccc` |
| Keypad | the desktop's panel at the right of the grid: 178 px wide, face colour, three 54×22 buttons a row on a 58/26 px pitch, 4 px margin, an 8 px gap after F24: F1–F24, Enter Reset Attn, SysRq PgUp PgDn, FldExit Field+ Field−, Help Print Clear; toggled from View > Keypad and the toolbar; its state saved with the session |
| Menu bar | File (Open session, Save session, Save session as, Recent sessions, Properties, Print screen, Close), Edit (Copy, Paste), View (Keypad, Colour map), Communication (Connect, Disconnect, Reconnect), Actions (Enter, Reset, Attention, System Request, Help, Clear, Keyboard map, Load keyboard map, Export keyboard map), Macro (Record, Play, Stop, Edit, Run ▸), Help. On the web the menu names are one grey line and the open menu's items the line below (`?menu=`) |
| Toolbar | Connect, Disconnect | Copy, Paste, Print Screen | Keypad, Keyboard Map |
| Status bar | message, indicator (`TLS`, cursor), `SYSTEM: state` (green connected, red not) |
| Keys (default map) | F1–F12; Shift+F1–F12 = F13–F24; Enter; Right Ctrl = Field Exit; Keypad + / − = Field+ / Field−; Esc = Attention; Shift+Esc = System Request; Ctrl+R = Reset; Insert; Pause = Clear; Ctrl+Home = Home; Tab / Shift+Tab; arrows; Delete; Backspace. The map is kept with the session as changes from the defaults; `.qkm` import/export is the desktop's file |
| Hotspots | protected text only: `Fnn=` labels send the key, `More...` Page Down, `Bottom` Page Up; shown only as the hand pointer |
| Selection | drag selects a rectangle of cells; Copy copies it as text, one line per row, trailing blanks dropped; Paste types at the cursor |
| Print screen | one-page A4 landscape PDF, the console font traced as vector rectangles, colours and attributes of the display |
| Macros | record keys and screen waits, play, stop, edit (a screen position picked with the mouse); `.qsm` files are the desktop's |
| Session files | `.qsd`: title, system, host, telnet port, TLS, screen size, CCSID (37, 273, 285, 297, 500, 1140), device name, colour map, auto connect, auto sign-on, hotspots, keypad visible, keymap changes, macros. Download and upload on the web, so one file serves both clients |
| Auto sign-on | one-time QAP `TelnetTicket` (256 random bits, 64 hex characters in the RFC 4777 password variable), wiped once sent, never logged |
| Connection | TN5250E to port 23, or 992 with TLS; device name or `*DFT`; the Certificate Not Trusted page for an untrusted certificate; `QAD1002`/`QAD1004` on failure |
| Script | the session page is the only page with script: a WebSocket carrying screen deltas and keys; the `<canvas>` is the image's exact size and draws with the same font. Without script the server renders the screen to PNG with the same `render()` and the keypad posts the key |

Golden check: the server's PNG of the fixture screens must be byte-identical to `shared/img/5250-24x80.png` and the others in this zip, which were rendered from the console font with the rules above.

### Feature parity table

| Desktop (1.1.0) | Access for Web | Page(s) | Status |
|---|---|---|---|
| Session Manager, `.qsd` files, Recent sessions, Properties | saved sessions per user, `.qsd` download/upload, Properties | `5250-config`, `5250-properties` | parity |
| Start 5250 Display: system, size, CCSID, device, TLS, colour map | same fields and the same CCSID list | `5250-config` | parity |
| Keyboard map dialog, Load / Export `.qkm` | same | `5250-keymap` | parity |
| Macros: Record, Play, Stop, Edit, Run, `.qsm` | same (script page) | `5250-macros` | parity |
| Hotspots, Copy/Paste, Print Screen PDF, Keypad, Colour map | same | `5250-session*` | parity |
| Verify Connection | same checks, optional test sign-on | `5250-verify` | parity |
| Sign On, Sign Off, Change Password, password expiry warning | sign-on page; Change password page; warning as a message row after sign-on | `signon`, `change-password` | parity |
| 5250 printer sessions (`.qsp`), received output | a browser cannot be a printer device | — | not applicable |
| Run Command: history 50, Run, Check, Prompt, Cancel, severity, job line, own connection | same; Cancel posts to the running command's job | `run-command`, `command-prompt` | parity |
| Prompt: definition form, lists with +, Additional Parameters (F10), ?, Show Command String, OK checks first | same | `command-prompt` | parity |
| Printer Output: filters, 25/50/100 a page, columns, sort page, Refresh / Auto-refresh, Open (SCS viewer, PDF/text), Save as PDF/Text, Print, Hold now / end of page, Release, Delete, Move (to top), Properties, Output Queues | same; Print = download the PDF; the viewer is Navigator's spooled file viewer | `printer-output`, `splf-props`, `nav/spooled-view` | parity |
| Run SQL: script, run statement / selected / all, stop at first error, options (schema, naming, commit level), Commit / Rollback, results 500 + Fetch More / All, NULL grey `-`, exact decimals, hex for binary, messages with SQLSTATE/SQLCODE, Cancel, History 100, `.sql` files, own connection | same; a result set is a tab (page) | `run-sql`, `run-sql-result-2` | parity |
| Schemas: pattern, Tables / Views / Aliases / Physical / Logical files 100 at a time, columns with Label and CCSID, View Data, Copy Name, Transfer from / to | same | `tables`, `nav/table-columns`, `table-view` | parity |
| Data Transfer: from and to the system, CSV / TSV / XLSX / ODS / fixed width, exact decimals, progress + Cancel, `.qdx` requests, `qsaccess run` | same formats both ways, `.qdx` import/export; unattended runs stay with the desktop CLI | `copy-export`, `copy-import`, `my-requests` | parity (CLI n/a) |
| Integrated File System: tree, 500 a page, `*` `?` pattern, links and broken links, New Folder, Rename, Cut / Copy / Paste on the system, Delete with count, Properties, Edit, Open, Download / Upload queue, binary / text with CCSID and line endings, `.qsapart` + Retry, drag and drop | same except drag and drop from the desktop; the browser's multiple-file upload instead | `files`, `file-props`, `file-edit` | parity (drag/drop n/a) |
| Messages: own / QSYSOPR / any queue, 100 a page, inquiry bold, second-level, Reply, Remove (asks twice for an unanswered inquiry), Send to user or queue, watching, Message Arrived, Messages waiting indicator | same; watching = a `Messages waiting: N` line in the header on every page and a Message Arrived page on the next request (no push without script) | `messages`, `message-arrived` | parity (polling) |
| 5250 Console and Control Panel (phase 10, not shipped) | QS/MC's terminal window | `qsmc/terminal`, `qsmc/control-panel` | elsewhere |
| Installers, silent install, MIME types | — | — | not applicable |

Messages the web product shares with the desktop (same IDs, same text, from `qsa-msg`): `QAD1002`, `QAD1004`, `QAD1010`, `QAD1011` (connection and trust), `QAD5001`, `QAD5010`–`QAD5013`, `QAD5020` (Run Command), `QAD6001`–`QAD6021` (Printer Output), `QAD7001`–`QAD7032` (connections, Run SQL, transfers), `QAD8020`–`QAD8022`, `QAD8040` (files). Reuse the text verbatim.


## Build order

One step = one commit, ending with the page matching its golden file.

### Group 1 — frame, sign-on, policies, home

| Step | Golden page(s) | Route(s) | Backend |
|---|---|---|---|
| 1 | `web/signon.html`, `web/change-password.html` | `GET/POST /signon`, `/password` | user profile auth through the QAP sign-on; password expiry warning (`PasswordStatus`) as a message row; Change password with the system's own messages; any enabled profile may sign on unless policy "Sign on to Access for Web" is Deny |
| 2 | `web/policies.html` | `GET/POST /policies?p=` | policy store (function key → Allow/Deny per profile or group, kept in `QUSRSYS`); effective view for a user including its groups; `*SECADM` required; every other page calls `policy(user, function)` before rendering an action |
| 3 | `web/home.html` | `GET /home` | welcome line with last sign-on and counts (messages, spooled files, active jobs); section links; the only page allowed a 32×32 glyph per link |
| 4 | `web/preferences.html` | `GET/POST /preferences` | per-user settings in `QUSRSYS`: rows per page, date/time format, time zone, language, default schema, naming, output format, max rows, 5250 defaults, home layout |

### Group 2 — 5250 session (the JavaScript page)

| Step | Golden page(s) | Route(s) | Backend |
|---|---|---|---|
| 5 | `web/5250-config.html`, `web/5250-properties.html`, `web/5250-verify.html` | `GET/POST /5250`, `/5250/<id>/properties`, `/5250/verify` | `.qsd` session files per user (the desktop's format: `qsa-config::qsd`), Start / Properties / Download / Delete, upload of a `.qsd`; Verify Connection with the desktop's checks |
| 6 | `web/5250-session.html` (+ `-white`, `-x2`, `-wide`, `-disconnected`, `-wait`) | `GET /5250/session/<id>?menu=&scale=`, `GET /5250/session/<id>/screen.png`, `wss://…/5250/ws/<id>`, `POST /5250/session/<id>/key` | the `tn5250` crate from the desktop repository as the server-side client (TN5250E, device name, 24×80 / 27×132, CCSID, TLS, auto sign-on ticket); `render()` from `qsa-ui::console` ported to the server for the PNG and the Print Screen PDF; WebSocket frames = screen deltas, OIA and cursor; the browser `<canvas>` draws with the same font and rules (see Parity); keypad, menu bar, toolbar and status bar exactly as the desktop window; keypad buttons post the AID key when script is off |
| 6a | `web/5250-keymap.html` | `/5250/<id>/keymap` (+ load/export `.qkm`) | the desktop's `qsa-config::keymap` defaults and `.qkm` format |
| 6b | `web/5250-macros.html` | `/5250/<id>/macros` (+ `.qsm` upload/download) | the desktop's `qsa-config::qsm` format; record/play in the script page |

### Group 3 — database

| Step | Golden page(s) | Route(s) | Backend |
|---|---|---|---|
| 7 | `web/tables.html` | `/db/tables?s=&n=` | catalog of tables and views the user is authorized to in the chosen schema |
| 8 | `web/table-view.html` | `/db/<schema>/<table>?q=&col=&sort=&page=&fmt=` | paged SELECT (25 rows), Find, per-row Edit and Delete (confirmation), output format select HTML / CSV / PDF / Excel-compatible |
| 9 | `web/table-insert.html` | `/db/<schema>/<table>/insert`, `/db/<schema>/<table>/edit?key=` | generated form, one field per column, type and length shown in grey, NULL checkboxes for nullable columns, required marks for NOT NULL without default; Insert, Insert and add another |
| 10 | `web/run-sql.html`, `web/run-sql-result-2.html` | `GET/POST /sql`, `/sql/result/<n>` | the desktop's Run SQL: a script of statements, Run statement / selected / all, stop at first error, schema / naming / commit level, Commit / Rollback, a tab per result set, 500 rows + Fetch more / all, messages with SQLSTATE / SQLCODE, history 100, `.sql` open / save, download in CSV / TSV / XLSX / ODS / fixed width / HTML / PDF; policy distinguishes SELECT-only from any statement |
| 11 | `web/my-requests.html` | `/requests` (+ run/edit/copy/delete/share) | saved SQL and copy requests per user; Share with a group or `*PUBLIC` |
| 12 | `web/copy-import.html`, `web/copy-export.html` | `GET/POST /db/copy/to`, `/db/copy/from` (multipart) | the desktop's Data Transfer both ways: CSV / TSV / XLSX / ODS / fixed width, exact decimals, field mapping, append / replace / update, progress page + Cancel, `QAD7030`–`QAD7032`, `.qdx` requests shared with the desktop |

### Group 4 — files

| Step | Golden page(s) | Route(s) | Backend |
|---|---|---|---|
| 13 | `web/files.html`, `web/file-props.html`, `web/my-files.html` | `/files?path=&pat=&page=`, `/files/props?path=`, `/files/shares` | the desktop's Integrated File System window: folder tree, 500 entries a page with `*` `?` pattern, links with targets (broken in red), New folder, Rename, Cut / Copy / Paste on the system, Delete with the count, Properties, Edit, Open, transfer queue with binary / text (CCSID, line endings), `.qsapart` partial downloads and Retry (HTTP Range), multiple-file upload, existing-file policy |
| 14 | `web/file-edit.html` | `GET/POST /files/edit?path=` | text files only (CCSID-aware), Courier textarea, Save / Save as / Reset; refuses files over a configured size and binary files |

### Group 5 — print, messages, jobs

| Step | Golden page(s) | Route(s) | Backend |
|---|---|---|---|
| 15 | `web/printer-output.html`, `web/splf-props.html`, `web/printers.html` | `/print?u=&q=&s=&ft=&ud=&page=&rows=&auto=`, `/print/props/<id>`, `/print/printers` | the desktop's Printer Output: filters, 25/50/100 a page, page sort, Refresh / Auto-refresh (`<meta refresh>`), Open in the viewer, Save as PDF / text, Hold now or at end of page, Release, Delete, Move (to top), Properties (every field), summary `QAD6006`; Output Queues = Navigator's page |
| 16 | `web/messages.html`, `web/message-arrived.html`, `web/send-message.html` | `/messages?q=&page=`, `/messages/arrived`, `/messages/send` (+ reply/remove POSTs) | the desktop's Messages: own / QSYSOPR / any queue, 100 a page newest first, inquiry rows bold, second-level text, Reply (empty = default), Remove (asks twice for an unanswered inquiry), Send to user or queue, `Messages waiting: N` in the header of every page, Message Arrived page per the watch options |
| 17 | `web/my-jobs.html`, `web/server-jobs.html` | `/jobs?s=`, `/jobs/server` | `WRKUSRJOB` for the signed-on user including completed jobs with output; Job log and Properties link to Navigator's pages; End via the End Job confirmation; server jobs = the `ACCESSWEB` instance and the host server jobs serving this session, End on them requires the "End server jobs" policy and `*JOBCTL` |

### Group 6 — commands and download

| Step | Golden page(s) | Route(s) | Backend |
|---|---|---|---|
| 18 | `web/run-command.html` | `GET/POST /cmd` | run a CL command through the remote command host server under the user's profile, interactively (wait) or `SBMJOB` to a job queue; result area shows the completion message and job; recent commands per user |
| 19 | `web/command-prompt.html` | `GET/POST /cmd/prompt?cmd=` | the web F4: read the command definition (`QCDRCMDD`-equivalent), render keyword / parameter text / value control / choices per parameter, "+" expands list parameters, Show command string rebuilds the string, Run, Save to My commands |
| 20 | `web/my-commands.html` | `/cmd/saved` | saved commands per user with Run as interactive/batch and Share |
| 21 | `web/download.html` | `/download` | static list of client pieces from `/QS400/ProdData/Access/Web/download`: printer drivers, CUPS PPDs, terminal font, keyboard map, CA certificates, documentation, connection details |

Messages this product owns (message file `QUSRSYS/QZXWMSG`): `XWA0001` policy denies function, `XWA0010` session saved, `XWA0020` request saved, `XWA0030` copy completed with N rows and M errors, `XWA0040` file uploaded, `XWA0050` command submitted to job queue; everything else surfaces the underlying command's message.

## Fixture data

Load the prototype's sample data as fixtures: user `CWELLMAN` (Cody Wellman, `*SECOFR`, group `ADMINS`), schema `HLINV` with `ASSETS` (12 sample rows in `table-view`), home directory `/home/CWELLMAN` with the files listed in `web/files.html`, eight spooled files, three messages with one inquiry, saved sessions "QSHQ400 operator" / "QSHQ400 default" / "QSBR1400 branch", the policy matrix for `WEBUSER` / `WEBUSERS` from `web/policies.html`.

## Product-specific acceptance

- With script disabled every page except the 5250 session renders and operates exactly as with script; the 5250 session still shows the last screen and accepts keys from the keypad row by POST.
- A function set to Deny in policies for the user's group disappears from the actions and is refused on direct POST with `XWA0001`.
- SQL, commands and file operations run under the signed-on user's profile; a job log on the server shows the user's name as current user, never `QTMHHTTP`.
- The 5250 WebSocket carries no password; auto sign-on uses a one-time ticket.
- `table-view` and `run-sql` cap at the user's max rows (default 500) and offer the full result as a download.
