# QS/400 Access for Web

The end user's web interface: HTTP server instance `ACCESSWEB` in
subsystem `QHTTPSVR` on port 2003 and, with TLS, 2006. Licensed program
`QS00AFW`. The approved design is the handoff
(`docs/ACCESSWEB_HANDOFF.md`), the style standard `docs/WEB_STYLE.md`,
and the golden pages under `net/accessweb/tests/golden/`.

## Crate

`net/accessweb` runs over [`qsweb`](HTTPADMIN.md), as Navigator does.

| Module | What |
|---|---|
| `env/` | `Env`: `System` (name, clock and zone, sign-on and sign-off, password change and rules, special authorities, class, groups, profile text, the profiles policies are kept for, the home page's counts, audit, message texts) and one trait per area: `policy` (`PolicyStore`), `prefs` (`PrefStore`), `session5250`, `db`, `files`, `print`, `msgs`, `jobs`, `cmd`, `download`. |
| `policy.rs` | Every function (`SECTIONS`, the keys as constants), the levels (user, groups, `*PUBLIC`), `resolve`, `policy(env, user, function)`. |
| `prefs.rs` | Every preference's key and default, `Prefs` (the stored values over the defaults, the date and time formats). |
| `msgs.rs` | `QUSRSYS/QZXWMSG`: the `XWA` messages and the desktop's `QAD` messages the product shows. `runtime/msgf/msgs/qzxwmsg.toml` is made from it (`QS400_UPDATE_MSGF=1 cargo test -p accessweb --test msgf`). |
| `routes/` | `dispatch` (session, CSRF, the expired-password gate, the `/nav` tree toggle), `Rq` (with `Rq::policy` and `Rq::allowed`), sign-on, home, the generic pages, `customize/` (Preferences, Change password, Policies), then one folder per area. |
| `fixture/` | The prototype's sample system: `QSHQ400`, US Eastern time, clock 10/29/26 14:03:40, user `CWELLMAN` (Cody Wellman, `*SECOFR`, group `ADMINS`), `WEBUSER` in group `WEBUSERS`, groups `WAREHOUSE` and `ADMINS`; one module per area with its data (`HLINV.ASSETS`' 12 rows, `/home/CWELLMAN`, 8 spooled files, 3 messages with one inquiry, 4 saved sessions), `text.rs` the system message texts. |
| `help.rs` | Help topics. |
| `transfer.rs` | The transfer queue of Browse files, per user (steps 13-14). |

`cargo run -p accessweb --bin accessweb-demo [PORT]` serves the product
over the fixture at `http://127.0.0.1:2003/qs400/accessweb/` (any
fixture profile, password `QS400`).

## Decisions

- Every action runs as the signed-on user (host servers, QAP, telnet),
  never as `QTMHHTTP`; on top of the command's own authority check, the
  product checks its policy for the function first. `Rq::policy`
  refuses with the Not authorized page, `XWA0001` and its policy row,
  and writes an `AF` entry (`need` = `policy <key>`); `Rq::allowed`
  hides an action the policy denies.
- Policies: a function is checked at the user profile, its groups (in
  the profile's order), then `*PUBLIC`. A Deny at any level denies; an
  Allow allows when nothing denies; a function nobody set is allowed
  (`Default`). "Change policies (administrator)" is the system's: Allow
  with `*SECADM` (`*ALLOBJ` does not give it), Deny without, Set by
  `System`; the radios of that row are not kept. The Policies page needs
  it. Apply keeps for the profile only the settings that differ from
  what it inherits from the levels below.
- The Policies select lists the user profiles of class `*USER`, then the
  group profiles, then `*PUBLIC` (another profile reached by `?p=` comes
  first); `Copy policies from...` (`/policies/copy?p=`) replaces the
  profile's own settings with another's.
- Policy keys (`policy.rs`): `signon`, `5250.start|save|autosignon|print`,
  `db.browse|write|select|sql|copyto|copyfrom|save|share`,
  `files.browse|upload|download|edit|delete|outside`,
  `print.text|pdf|own|move|others`, `msgs.display|send|inquiry|reply`,
  `jobs.display|end|server|endserver`, `cmd.run|prompt|saved|save`,
  `prefs.change`, `policies.change`.
- Sign-on: the system's 5250 rules; then policy `signon` (Deny: the
  sign-on page with `XWA0001`, an `AF` entry). A password in its warning
  period leaves `QAD4031` for the first page and on Change password
  while the session lasts. An expired password opens Change password
  only (`QAD4022`); once changed, the session ends and the sign-on page
  says `QAD4023`. A mismatched confirmation is `QAD4027`, nothing sent.
- The home page's "Last sign-on" is the profile's previous sign-on and
  the address of the previous Access for Web sign-on (kept with the
  preferences as `_from`). Its section links follow the policies
  (`5250.start`, `db.browse`, `files.browse`, `print.text`,
  `msgs.display`, `jobs.display`, `cmd.run`).
- Preferences: General shows General, Database, 5250 and Home page (the
  golden page); each other tab (`?tab=database|5250|files|print|home`)
  its own group. A tab's post changes only its fields; a value outside
  a field's choices is `XWA0004`, nothing saved. Only values that differ
  from the defaults are stored. The default schema's default is the
  first schema offered. The date and time formats and the zone (System
  or UTC) apply to every date the product shows (`Rq::stamp`).
- The stores (`QUSRSYS`): policies in user index `QZXWPOL`, preferences
  in user index `QZXWPRF`, made with the product, owned by the system
  and `*PUBLIC *USE` (see The live server).
- The product's messages are `XWA0001`-`XWA0005`, `XWA0010`, `XWA0020`,
  `XWA0030`, `XWA0040`, `XWA0050`, `XWA0099`; the desktop's `QAD4022`,
  `QAD4023`, `QAD4027`, `QAD4031` in its own text. Area agents add the
  `QAD` messages their pages share with the desktop to `msgs.rs`.
- The tree: My home page, 5250 session, Messages and Download are leaves
  (`qsweb::frame::NavGroup::leaf`: a group whose one item has its own
  label); Customize holds Preferences, Change password and Policies, as
  the golden pages show.
- Areas not built answer `501` with `XWA0099` (Not available) in their
  tree group: `BLOCKED(rel)` in each `routes/<area>/mod.rs`.
- Files (steps 13-14): every page needs policy `files.browse`; a path
  outside the home directory `files.outside` (My files then lists the
  home directory only); Download and Open `files.download`, the editor
  `files.edit`, Delete `files.delete`, the upload `files.upload`.
  Browse files shows 500 entries a page (the rows select offers 10 to
  100), directories first then by name; `..` counts as an entry. Cut
  and Copy keep their paths in the session; Paste renames (cut) or
  copies on the system (`QAD8003`, `QAD8005`). Delete is confirmed
  with the count of objects a folder takes with it, and deletes from
  the inside out (`QAD8004`, or `QAD8006` where it stopped). The links
  that change something (Cut, Copy, the queue's Cancel, Retry,
  Replace, Skip) carry the session's form token as `?t=`.
- Transfers: the queue is per user, the 50 newest kept. Downloads are
  read a piece of 256 KiB at a time as the user and answer `Range`
  (one range, `If-Range` against the `ETag`), so a browser resumes an
  interrupted download; one that stops shows Failed with the reason
  (`QAD8021`) and Retry, which fetches it again. A text download
  converts from the file's CCSID to UTF-8 with CR LF or LF; a file
  tagged 65535 or with a NUL in its first 8 KiB is always binary.
  Open serves PDF, images and text inline (`Content-Security-Policy:
  sandbox`, `nosniff`), anything else as a download. Several files
  selected for Download wait in the queue, each fetched from its row.
- Uploads: the browser's multiple-file upload (qsweb's multipart, a
  request of at most 8 MiB, `BODY_MAX`). Binary keeps the bytes (a new
  file is tagged 1208 when they are UTF-8, else 65535); Text converts
  UTF-8 to the CCSID given with LF line endings (an unknown CCSID is
  `XWA0004`, nothing written). An existing file: Ask leaves the upload
  in the queue as `QAD8010` with Replace and Skip (its data kept),
  Skip skips it, Replace and Replace if newer replace it (an upload
  carries no time of its own, so it is newer). Done: `XWA0040`; a
  failed write stays in the queue with its data for Retry. The
  Download options are sent with the upload form and kept for the
  session; their default is the "Transfer files as" preference.
- The editor: text in the file's CCSID and its own line endings; Save
  writes it in place (owner, authority and CCSID kept), `XWA0042`; a
  file changed since it was opened (its time or size) asks Overwrite,
  Reload or Cancel (`QAD8011`); Save as... carries the text to a new
  path. A file tagged 65535, with a NUL in its first 8 KiB or over
  8 MiB opens read-only (`QAD8013`); over 64 MiB it is refused
  (`QAD8012`).
- Printer output (step 15): the page needs `print.text` or
  `print.pdf`; Hold, Release, Delete `print.own`; Move `print.move`;
  another user's files (`*ALL`, another user, their actions)
  `print.others`, then the system's own check (`*SPLCTL`, `*JOBCTL`).
  The list is newest first, a page of the preference's rows; a column
  heading sorts the page shown, as the desktop's does. A spooled file's
  id is `number-user-job-file-splnbr` (system names hold no `-`). One
  file's action answers its message (`QAD6002`-`QAD6005`); several
  files' answer `QAD6006` when any was refused, with each file's
  result in a table under it. Save as PDF and Save as text need one
  file (`XWA0045`). `OPN` files are `QAD6012`; a device type other than
  `*SCS` or `*LINE` is `QAD6013`.
- The viewer: Navigator's spooled file viewer page, served in this
  product at `/print/view/<id>` (`/pdf`, `/text`, `?save=1` to save),
  so it runs as the Access for Web user and under its policies whether
  or not Navigator is installed. Text pages are apart by form feeds.
- Files and Print add `XWA0041` (nothing selected), `XWA0042` (file
  saved), `XWA0043` (authority changed), `XWA0045` (one spooled file
  needed) and the desktop's `QAD6001`-`QAD6006`, `QAD6012`, `QAD6013`,
  `QAD8002`-`QAD8006`, `QAD8010`-`QAD8013`, `QAD8020`-`QAD8022` in its
  own text. `QAD8040` (the desktop's private link) has no use on the web.
- Printers: the writers, their state and output queue; Print to opens
  Printer output with the printer's queue chosen for Move, Output lists
  its queue's files.

## Golden comparison

`tests/common` drives the handler with a browser in memory over the
fixture; `proto()` maps every served URL to its prototype file, and
`tests/golden.rs`' `ROUTES` holds every route of the handoff's build
order with its golden page (`STATES`: the goldens that are another
state of one of them). `assert_golden(served, file, fixes)` drops the
CSRF field, ignores whitespace between tags and compares the rest.
Every page's footer and the sign-on notice carry the current release
(`V1R0M0` in the zip, rewritten to `V1R1M0` when copied: data). Fixes
are real defects of the golden page, or data:

| Page | Fix | Why |
|---|---|---|
| every signed-on page | header `User: QSECOFR` → `User: CWELLMAN` | data: the handoff's fixture user is `CWELLMAN`, whose name the home page shows |
| `web/signon.html` | form `action` `home.html` → `signon.html` | the form posts to the sign-on route; home follows by redirect |
| `web/change-password.html` | the current item is Change password, not Preferences | the tree marked another page |
| `web/change-password.html` | `CPF1107 - Password for user profile CWELLMAN expires in 5 days.` → `QAD4031 - Password for CWELLMAN on QSHQ400 expires in 5 days.` | `CPF1107` is a sign-on error (no user given); the warning is the desktop's `QAD4031`, in its words |
| `web/change-password.html` | one `<form>`, not two nested | a form inside a form is not valid HTML |
| `web/policies.html` | a `Sign on` section with "Sign on to Access for Web" ahead of 5250 (two rows: the stripes stay) | the handoff's step 1 names the policy; the page had no row for it |
| `web/policies.html` | Apply, OK, Cancel, Copy policies from... get names `apply`, `ok`, `cancel`, `copy` | four nameless buttons in one form cannot be told apart |
| `web/5250-config.html` | one `<form>` (multipart, to `/5250/session/new`), not two nested; the buttons get names `start`, `save`, `verify` | a form inside a form is not valid HTML; three nameless buttons cannot be told apart |
| `web/5250-verify.html` | one `<form>`, not two nested | not valid HTML |
| `web/5250-session*.html` | the page's one `<script src=".../5250/script.js" data-session data-scale data-scheme>` where the prototype's comment says it goes | the handoff's script; the comment is its placeholder |
| `web/5250-session-x2.html` | `Scale: <a>1x</a> <b>2x</b>` | the 2x page showed 1x as current |
| `web/5250-session-white.html`, `-wide`, `-disconnected`, `-wait` | the picture's file name is `screen.png` of the one route | data: one route draws every state; each picture's bytes are checked against its own PNG (`tests/s5250_look.rs`) |
| `web/5250-session-disconnected.html` | `QAD1004 - The connection to QSHQ400 was closed by the system at ...` → `QAD1005 - Session to QSHQ400 ended by the system.` | `QAD1004` is the desktop's time-out; a session the system ends is its `QAD1005`, in its words |
| `web/5250-session-wait.html` | status bar cursor `24/006` → `06/049` | data: the cursor of the sign-on display the picture shows |
| `web/5250-keymap.html` | the grouped rows (F1 ... F12, Shift+F1 ... Shift+F12, the arrows) have no select; the form is multipart; Add, Load, Export get names | a select with nothing selected posts its first choice (Enter) and would remap twelve keys at once; an upload needs multipart; nameless buttons |
| `web/5250-macros.html` | SIGNON-DEV `4` steps → `6`; one `<form>`, not two nested; Record gets name `record` | data: the editor shows six steps; not valid HTML; nameless button |
| `shared/img/*.png` | none: byte for byte | the encoder is a zlib-ng-compatible greedy deflate (good 4, insert 6, nice 32, chain 32, memory level 9; `s5250/look/deflate.rs`) with the usual PNG filter heuristic, found by comparing the prototype's streams with zlib-rs at every setting |

## 5250 session (steps 5-6b)

- `s5250/look/`: the console's font (`lic/console/font/qs400-9x16.txt`,
  compiled in), the palette and cell rules ported from the desktop's
  `qsa-ui/src/console.rs` (`render.rs`), the PNG writer and its deflate,
  and Print Screen (`pdf.rs`: one A4 landscape page, each run of lit
  pixels a filled rectangle). `s5250/screen.rs`: the screen the pages
  draw, its status row (`TLS`, `X SYSTEM`, `Not connected`, `RR/CCC`).
- `s5250/files/`: the desktop's `.qsd`, `.qkm` and `.qsm` ported from
  `qsa-config` (TOML over `toml`, unknown keys kept), so a file downloaded
  here opens in the desktop and back. The macro editor shows a macro's
  steps one a line (`type "X"`, `key NAME`, `wait screen "X" at R,C`,
  `wait unlock`, `pause MS`, `prompt "L"`, `; comment` kept as the step's
  `comment` key).
- Pages: `/5250` (saved sessions, Start 5250 display, a `.qsd` opened,
  Download, Delete after asking), `/5250/<n>/properties`, `/5250/verify`
  (`?sys=`), `/5250/session/<n>` (`?menu=`, `?scale=`, `?play=`, `?save=1`;
  `screen.png`, `screen.txt`, `print`, `copy`, `paste`, `key`),
  `/5250/ws/<n>`, `/5250/<n>/keymap` (`/export`, `/json`),
  `/5250/<n>/macros` (`/<m>/download`, `/<m>/delete/confirm`),
  `/5250/script.js`. Number 0 is a session started without saving, kept
  for the sign-on only.
- Without script: the toolbar and keypad post; Edit > Paste types at the
  cursor, Copy shows the screen's text; the picture is the server's PNG.
  Macros record (keys, typing, a `wait unlock` after each key that goes to
  the system; typing into a nondisplay field is kept as a prompt) and play
  on the server; a prompt step needs the script.
- With script (`assets/s5250.js`, the one script of the web products):
  the picture becomes a canvas of its size drawn with the same font and
  rules; keys go through the session's keyboard map; hotspots, selection,
  Copy and Paste. The WebSocket (`qsweb::ws`) carries the whole screen,
  then the rows that changed, one style letter a cell; a nondisplay
  field's characters never go out; no password crosses it.
- Policies: `5250.start` for the area, `5250.save` for every change,
  `5250.autosignon` (the box disabled, a start without the ticket),
  `5250.print` for Print Screen.
- Messages: `XWA0010` session saved, `XWA0011` deleted, `XWA0013` macro
  saved, `XWA0014` deleted; the desktop's `QAD1002`-`QAD1006`, `QAD1010`,
  `QAD1011`, `QAD1020`, `QAD1021`, `QAD2001`, `QAD2002`, `QAD2010`,
  `QAD2020`, `QAD2021` in its words.
- Live (`work/qsjob/src/work/httpafw/s5250.rs`): saved sessions are kept by
  the job under `/www/accessweb/sessions/<USER>/<n>.qsd` (the web's last
  use and macros in an `[accessweb]` table, left out of downloads),
  keyboard maps in `keymaps/<name>.qkm`. A display is a TN5250E client
  (`ui/tn5250`) of the local Telnet server on 23, or 992 with TLS (the
  chain to `*SYSTEM`'s CAs checked); automatic sign-on takes a one-time
  QAP ticket as the signed-on user (the storage service accepts the
  `ACCESSWEB` job running as its user) and sends it as the RFC 4777
  password variable with an empty seed (`Config::ticket`). Verify
  Connection checks the local ports.
- `BLOCKED(rel)`: a session to another system is refused with `QAD1002`
  (`httpafw/s5250.rs`); the display shows and types CCSID 37, the
  session's CCSID goes to the server as `CODEPAGE` only.

QEMU step 154 (`tools/conformance/src/afw5250.rs`) signs a new profile on
over 2003, saves a session, downloads its `.qsd`, starts it (the sign-on
display in `screen.txt` and `screen.png`, the WebSocket's first screen),
signs on at that display through Paste, Field Exit and Enter to the Main
Menu, prints the screen, disconnects (`QAD1006`), deletes (`XWA0011`).
| `web/tables.html` | the Objects select says `All` | the page lists every kind; `Tables` would list tables only |
| `web/tables.html` | `ASSETS`, `ASSETS_BY_LOC`, `ASSETLF1` have 12 rows, not 412 | data: the handoff's `ASSETS` has 12 rows, and the view and logical file are over it |
| `web/table-view.html` | the Output select and Go inside `<form method="get">` | a select outside a form sends nothing |
| `web/table-view.html` | the columns `COST`, `RACK_U`, `NOTES`, `CHANGED_TS` in the header, the rows and the Find and Sort by selects | data: `ASSETS` has twelve columns (`web/table-insert.html`, `nav/table-columns.html`) |
| `web/table-view.html` | `Page 1 of 1`, no Next link, `12 entries` | data: twelve rows are one page |
| `web/run-sql.html` | `12 rows fetched.` → `12 rows fetched in 0.041 seconds.` (and the 4 rows) | the system's own `SQL7958` text (`QSQLMSG`) |
| `web/run-sql.html` | the UPDATE's `SQL7956` → `SQL7957`, `1 row updated in ...` → `1 rows were updated in ...` | `SQL7956` is an insert's; the text is `SQL7957`'s |
| `web/run-sql.html` | Fetch more and Fetch all in a `<form method="post">`, named `fetch` and `fetchall` | buttons outside a form post nothing |
| `web/run-sql-result-2.html` | sort keys `?sort=l` → `?sort=location` | the result pages sort by the column's name, as page 1 does |
| `web/my-requests.html` | Run and Edit of the copy requests → `copy-import.html`, `copy-export.html` | a copy request opens its copy page, not Run SQL |
| `web/copy-import.html` | `Or file on this system` empty; Browse... named `browse` | data: the file came from this computer; a nameless button cannot be told apart |
| `web/copy-import.html`, `web/copy-export.html` | the Table select lists `HLINV`'s six tables | data: the schema's tables (`web/tables.html`) |
| `web/copy-import.html` | Copy, Preview 10 rows, Save request (.qdx), Cancel named `copy`, `preview`, `save`, `cancel` | four nameless buttons in one form cannot be told apart |
| `web/copy-export.html` | one `<form>`, the buttons inside it, named `copy`, `save`, `cancel` | a form inside a form is not valid HTML; the buttons were outside both |

## Database (steps 7-12)

- `env/db.rs` is the database as the user: the catalog (schemas,
  tables and the like, columns with key, default and choices),
  connections of a page's own (at most 16, `QAD7003`), one statement
  run with parameter rows, Fetch, Commit and Rollback, Cancel, and the
  saved requests and history. Values are text (`Val`): decimals are
  never floating point, NULL is its own value, binary data is hex.
- The table pages make their SQL in `data/sqltext.rs` with delimited
  names and parameter markers; nothing typed becomes SQL text.
- Tables: the filter (`sp`, `s`, `t`, `n`, `sort`) stays with the
  session, so paging keeps it; 100 a page; Columns links to
  Navigator's page on 2002 (2005 over TLS); Copy name shows the
  qualified name to copy.
- A table's rows: 25 a page (the user's rows per page), at most the
  user's maximum rows (`QAD7011` says so); Edit and Delete per row only
  for a table with a key; another output downloads every row selected
  (CSV, PDF, the Excel-compatible XML spreadsheet).
- Insert and Edit: required = NOT NULL without a default; a NULL box
  for nullable columns, ticked at first for those that are not text; a
  text column over 100 long is a text area, NULL when left empty;
  values the system makes (`CURRENT TIMESTAMP`, identities) are left
  out; dates in the user's date format; a single character key ending
  in digits suggests the next one; today's date for dates.
- Run SQL keeps its connection, options, script and results with the
  session (closed, its changes rolled back, once the session ends).
  Run statement runs the one at the posted cursor (`pos`, 0 without
  script); Run selected needs a selection (`sel0`, `sel1`), else
  `XWA0021`. A statement that only reads (`SELECT`, `VALUES`, `WITH`)
  needs policy `db.select`, any other `db.sql`; a script is refused
  whole before anything runs. Under a commit level the options cannot
  change while changes are pending (`SQL0752`, `QAD7021`). The first
  result is on `/sql`, the other queries' on `/sql/result/<n>`.
  Recent `.sql` files are the session's.
- Requests and the history are kept per user (the running system:
  user index `QUSRSYS/QZXWPRF` through the service's `web_store`,
  keys `_q...` and `_h...` of the profile, the shared ones listed
  under `*SHARED`). Run of a request that only reads runs it; Edit opens it;
  Download gives `.sql` or the desktop's `.qdx` (`data/qdx.rs`, its
  version 2, read and written the same way); Share lists it for a
  group or `*PUBLIC` (policy `db.share`), only the owner changes it.
- Copies run off the request in a connection of their own: a progress
  page (`?job=`, refreshed) with Cancel, then `QAD7030` and `XWA0030`,
  `QAD7031` (the row and line) or `QAD7032`. Upload: one unit of work
  under Read committed (without commitment control, `QAD7034`, when the
  table cannot use it), batches of 100; Stop rolls everything back,
  Skip keeps the others and names the first ten. The mapping is by
  name, then each source column to the next target column whose type
  takes its sample value. Update matching keys updates by the key and
  inserts the rest; it has no `.qdx` form (the desktop's requests have
  append and replace only), so saving it is `XWA0004`.
- `BLOCKED(rel)`: Copy data to table from a file on this system (the
  IFS) waits for the Files area's trait; refused with `XWA0099`
  (`routes/db/copy.rs`).
- The product's messages added: `XWA0021`-`XWA0027`; the desktop's
  `QAD7003`, `QAD7010`, `QAD7011`, `QAD7020`, `QAD7021`, `QAD7030`-
  `QAD7032`, `QAD7035`, `QAD7040` in its own text.
| `web/files.html` | the Path line is a `<form method="post">` of its own; Go and Paste get names `go`, `paste` | its fields and buttons sat in no form: nothing could be sent without script |
| `web/files.html` | each checkbox `name="s"` gets `value="<entry name>"` | a checkbox without a value does not say what it selects |
| `web/files.html` | the `..` row has no checkbox and only Open and Properties | renaming, cutting, copying or deleting the parent through its `..` row is not an action on this folder |
| `web/files.html` | `old-link`'s row before `reports`' (the stripes stay) | data: entries sort directories first, then by name |
| `web/files.html`, `web/file-edit.html` | `invload.clle` is `2,033` bytes, not `1,844` | data: the size of the text the editor's golden shows |
| `web/files.html` | Download, Cut, Copy, Delete..., New folder... get names `download`, `cut`, `copy`, `delete`, `mkdir` | nameless buttons in one form cannot be told apart |
| `web/files.html` | `Failed - QAD8021 not authorized`: the ID a link | message IDs are links everywhere |
| `web/files.html` | Text, not Binary, checked for the upload and for downloads | data: the fixture user's preference "Transfer files as" is Text (its default) |
| `web/file-edit.html` | Files open in the tree, Browse files current | the editor is part of Browse files, as Properties is; the tree marked none |
| `web/file-edit.html` | Save, Save as..., Reset, Cancel get names `save`, `saveas`, `reset`, `cancel` | nameless buttons in one form cannot be told apart |
| `web/printer-output.html` | each checkbox `name="sN"` gets `value="<spooled file id>"` | a row's number names another file once the list has changed |
| `web/printer-output.html` | Auto-refresh `30` not selected | data: Auto-refresh is off until chosen; `?auto=30` selects it and the page carries `<meta http-equiv="refresh" content="30" />` |
| `web/printer-output.html` | Hold, Release, Delete..., Move, Save as PDF, Save as text get names `hold`, `release`, `delete`, `move`, `pdf`, `text` | nameless buttons in one form cannot be told apart |
| `nav/spooled-view.html` (served as `/print/view/<id>`, content compared) | crumb `Access for Web > Print > Printer output > INVRPT`, `Back to Printer output` | Navigator's page in this product's frame |
| `nav/spooled-view.html` | `< Previous` grey on page 1, `Next >`, View as PDF and Download text real links | the golden's `#` links lead nowhere |
| `nav/spooled-view.html` | Hold and Release links, Delete a link to the confirmation | submit buttons in a GET form cannot hold a file; deleting is confirmed |

## The live server

Access for Web is licensed program `QS00AFW` `*BASE` (docs/LICPGM.md),
and HTTP server instance `ACCESSWEB`, off until started:
`STRTCPSVR SERVER(*HTTP) HTTPSVR(ACCESSWEB)` starts job `ACCESSWEB` in
`QHTTPSVR` under `QTMHHTTP` (role `HTTPAFW`,
`work/qsjob/src/work/httpafw/`); `ENDTCPSVR ... HTTPSVR(ACCESSWEB)`
ends it. Without `QS00AFW` the instance does not exist (`TCP1A06`). It
listens on 2003 and, with TLS, 2006 (the certificate `*SYSTEM` assigns
`QS400_HTTP_SERVER_ACCESSWEB`, a shipped application ID, else the
system's own); `CHGHTTPA`'s bag moves them (`AFWPORT`, `AFWTLSPORT`).
The `ADMIN` job makes its registry entry and `/www/accessweb` once
`QS00AFW` is installed (autostart off); HTTP Server Administration
lists it only then.

`QS00AFW`'s `*BASE` code load is `QUSRSYS/QZXWMSG` (`*MSGF`) and the
two stores, user indexes `QUSRSYS/QZXWPOL` (policies) and
`QUSRSYS/QZXWPRF` (preferences), made by `lic/osobjects`' products.rs
owned by the system, `*PUBLIC *USE` (variable entries, a 30-byte key: the profile in
10, the function or preference key in 20; the value after it). Only the
job `ACCESSWEB` under `QTMHHTTP` reads and changes them, through
the storage service (`web_store`, `lic/slic/src/stgsvc/afwstore.rs`, as the system: in the QEMU
run the restored indexes belonged to `QSYS`, so the job could not write them itself); everything else runs as the
signed-on user, the job swapped to the profile handle taken at
sign-on (`work/qsjob/src/work/httpafw/`, over `web/` as Navigator).
`DLTLICPGM` removes the objects (`QUSRSYS` stays), `RSTLICPGM` brings
them back.

| Part | Live |
|---|---|
| System | `web::system::Core` (name, special authorities, class, groups, audit `PW`/`AF`, message texts: `XWA`/`QAD` from `QUSRSYS/QZXWMSG`); sign-on with the 5250 rules (`sign_on`; an expired password signs on to Change password only, no handle), `QSYGETPH`; `QSYCHGPW` (`sec_api ChangePassword`); `QPWDMINLEN`, `QPWDMAXLEN`, `QPWDRQDDIF`, `QPWDLVL`; the zone from `QTIMZON` (`sysval::tz`); the profile's text and the profiles (`QSYRUSRI`, `QSYLAUTU`, a group as Navigator decides); the counts: `msgq_list QUSRSYS/<user>`, `splf_list <user>`, the user's active jobs |
| Policy, Prefs | `web_store` (the profile's entries, replaced when given) on the two stores; the schemas Preferences offers: the libraries the user may list, `QGPL` and those without a `Q` name |
| The areas | not built: `httpafw/areas.rs` (`NotYet`); their routes answer `XWA0099` |
| Policy, Prefs | `usridx_find` (range of the profile's keys) and `usridx_add` on the two stores, as the job; the schemas Preferences offers: the libraries the user may list, `QGPL` and those without a `Q` name |
| Database | `httpafw/db.rs` (`LiveDb`): every statement through QAP's `Sql` lane (`qapsql::Lane`) with the job swapped to the user's profile, a lane per connection whose embedded scopes and commitment definition (`*SQLAFWn`) are its own; the catalog from `QSYS2` (`SYSSCHEMAS`, `SYSTABLES`, `SYSTABLESTAT`, `SYSCOLUMNS`, `SYSCST`, `SYSKEYCST`); Cancel asks the engine to end the running statement (`qap::sql::ask_cancel`); requests and the history in `QUSRSYS/QZXWPRF` through `web_store` (keys `_q<id>.<n>`, `_h.<n>`; shared ones listed under `*SHARED` as `_s<owner>.<id>`); columns have no choices (check constraints are not read) |
| Policy, Prefs | `usridx_find` (range of the profile's keys) and `usridx_add` on the two stores, as the job; the schemas Preferences offers: the libraries the user may list, `QGPL` and those without a `Q` name |
| Files | `httpafw/files.rs` (`LiveFiles`): the storage service's IFS (`stg::remote::RemoteIfs`) with the job swapped to the user for each call, so its authority checks are the user's, as the file host server's are; errors the `errno`'s `CPFA0xx` message. The home directory is the profile's `HOMEDIR`. A file is written in place when it exists, else made with `O_CCSID` and the CCSID given. My files is the home directory alone (QS/400 serves no file shares) with the bytes under it (a walk of at most 20,000 entries) |
| Print | `httpafw/print.rs` (`LivePrint`): `splf_list` as the user (another user's files with `*SPLCTL`, `*JOBCTL` or `*ALLOBJ`), `splf_fetch` (`DSPSPLF`'s check and `SF` entry) decoded by work/spool (text, PDF), the actions as `HLDSPLF ... OPTION(*IMMED\|*PAGEEND)`, `RLSSPLF`, `DLTSPLF`, `CHGSPLFA ... OUTQ()` run as the user; the queues `Move to` offers are the `*OUTQ` objects the user may list; Printers are `WRKWTR`'s writers |
| The other areas | not built: `httpafw/areas.rs` (`NotYet`); their routes answer `XWA0099` |

- `BLOCKED(rel)`: the password approval program (`QPWDVLDPGM`) is not
  called from the web job: Change password is refused with `CPF22D3`
  while one is set (`httpafw/system.rs`).
- `BLOCKED(rel)`: Move with "top of queue" on the live system: no
  command or storage service request a web job may use puts a spooled
  file at the top of a queue (QAP's move does, on its connection job
  only); refused with `XWA0099` before anything moves
  (`httpafw/print.rs`). The fixture moves it.

QEMU step 153 (`tools/conformance/src/afwstep.rs`) saves `QS00AFW`
(`SAVLICPGM`), deletes it (`STRTCPSVR ... HTTPSVR(ACCESSWEB)` then
answers `TCP1A06`), restores it (`RSTLICPGM`, the three objects back),
starts `HTTPSVR(ACCESSWEB)`, signs on as `QSECOFR` over 2003, checks My
home page against `DSPNETA`, changes and uses a preference, denies a
new profile's sign-on with a policy (`XWA0001`) and clears it, refuses
a POST without the form field, shows Database's `XWA0099`, signs off,
signs on and loads My home page over TLS on 2006 (the cookie
`Secure`), and ends the instance.

QEMU step 155 (`tools/conformance/src/afwdbstep.rs`) starts the
instance, signs on as `QSECOFR` over 2003, makes schema `AFW155` with
table `ITEMS` and a row in Run SQL and reads it back (`SQL7956`), finds
the table in Tables, views its rows, inserts one with Insert record,
copies a CSV file of two rows in (`QAD7030`, `XWA0030`) and all four
out again as CSV with exact decimals, then signs on as a profile
without authority to the table and gets the database's `SQL0551` for
the same query (statements run as the user, not `QTMHHTTP`), and ends
the instance. Step 153 checks Not available on Files now.
QEMU step 156 (`tools/conformance/src/afwfiles.rs`) starts
`HTTPSVR(ACCESSWEB)`; profile `AFW156` with home directory
`/home/AFW156` signs on over 2003, Browse files lists its home
directory, a multipart upload writes a file `DSPAUT` says `AFW156`
owns (not `QTMHHTTP`), the download gives it back whole and from byte 6
(`206`, `Content-Range`), the editor opens it; then `QSECOFR` lists
printer output, opens a spooled file in the viewer, holds it
(`QAD6002`, `WRKSPLF` shows `HLD`), deletes it after the confirmation
(`QAD6004`), and the instance ends.

## Area ownership

Each area agent works in its own files; shared files take additive
changes only.

| Area | Owns |
|---|---|
| 5250 (steps 5-6b) | `src/env/session5250.rs`, `src/routes/s5250/`, `src/fixture/session5250.rs`, `work/qsjob/src/work/httpafw/s5250.rs` |
| Database (7-12) | `src/env/db.rs`, `src/routes/db/`, `src/fixture/db.rs`, `httpafw/db.rs` |
| Files (13-14) | `src/env/files.rs`, `src/routes/files/`, `src/fixture/files.rs`, `httpafw/files.rs` |
| Print, messages, jobs (15-17) | `src/env/{print,msgs,jobs}.rs`, `src/routes/{print,msgs,jobs}/`, `src/fixture/{print,msgs,jobs}.rs`, `httpafw/{print,msgs,jobs}.rs` |
| Commands, download (18-21) | `src/env/{cmd,download}.rs`, `src/routes/{cmd,download}/`, `src/fixture/{cmd,download}.rs`, `httpafw/{cmd,download}.rs` |
| Shared (additive) | `tests/common/mod.rs` (`proto` entries of the area), `tests/behavior.rs` (the area's `PENDING` line, its own tests), `tests/golden.rs` (its own tests), `src/help.rs` (its topics), `src/msgs.rs` (its messages, then the toml rewritten), `src/fixture/mod.rs` and `src/fixture/text.rs` (its parts and texts), this file's tables, `httpafw/mod.rs` (its `Live*` in the `Env`) |

## Steps

The handoff's build order; `tests/behavior.rs`' `PENDING` names the
routes not built yet, by area, and each step removes its own.

| Steps | Area | State |
|---|---|---|
| 1-4 | Frame, sign-on, change password, policies, home, preferences | done (golden-clean; live) |
| 5-6b | 5250 session | done (golden-clean, PNGs byte for byte; live; QEMU step 154) |
| 7-12 | Database | not built (`XWA0099`) |
| 5-6b | 5250 session | not built (`XWA0099`) |
| 7-12 | Database | golden-clean over the fixture (`tests/db.rs`) |
| 13-14 | Files | not built (`XWA0099`) |
| 15-17 | Print, messages, jobs | not built (`XWA0099`) |
| 13-14 | Files: Browse files, properties, My files, the editor, transfers | done (golden-clean; live; QEMU 156) |
| 15 | Print: Printer output, properties, the viewer, Printers | done (golden-clean; live; QEMU 156) |
| 16-17 | Messages, jobs | not built (`XWA0099`) |
| 18-21 | Commands, download | not built (`XWA0099`) |
