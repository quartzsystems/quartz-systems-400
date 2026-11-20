# QS/400 web interfaces — style sheet in one page

Applies to QS/400 HTTP Server Administration (port 2001), Navigator for QS/400 (2002 / 2005), QS/400 Access for Web, and the QS/MC Management Console. One stylesheet, `shared/qs400.css` (177 lines and one addition: `@view-transition { navigation: auto; }`, so moving between pages fades instead of blanking where the browser supports it). The reference page `shared/components.html` shows every pattern once. The test for any new page: would it look at home in a 2006 browser on a 1024×768 CRT, with CSS turned off, with no script? If not, it is wrong.

## Colour

| Use | Value |
|---|---|
| Page background | `#ffffff` |
| Body text | `#000000` |
| Secondary text (descriptions, property labels, footer) | `#555555` / `#777777` |
| Links | browser default blue `#0000cc`, underlined; visited `#551a8b` |
| Header band | `#2b3a55`, white text, 40 px tall |
| Quartz accent `#00d992` | once per page only: the 2 px rule under the header band. Never a button, never a fill, never a status colour |
| Navigation column / wizard step column | `#e8e8e8` / `#f0f0f0`, 1 px `#c0c0c0` right border |
| Table header row | `#e0e0e0` |
| Alternate table rows | `#f0f4f8` (very light blue); other rows white |
| Table borders | `#c0c0c0` outer, `#d8d8d8` inner |
| Informational message | `#ffffcc` |
| Completion message | `#eeffee` |
| Error message | `#ffdddd` |
| Status text, good | `#007700` — Running, Active, Started, Operating, RUN, *ENABLED, Released, Varied on |
| Status text, bad | `#cc0000` — Stopped, Ended, Error, MSGW, *DISABLED, Held, Varied off, Not applied, Power off, No connection, Not activated |
| Utilisation bar fill | `#6688aa`; `#cc0000` at 70 % and above |
| 5250 screen / terminal window | `#000000` ground, `#00ff66` text (or white ground, black text, user-selectable) |

Nothing else is coloured. A status that is neither good nor bad is black.

## Type

| Element | Face | Size |
|---|---|---|
| Body, forms, navigation | Verdana, Arial, Helvetica, sans-serif | 9 pt body, 8 pt tables / nav / labels / messages, 7 pt footer |
| Page title `h1` | same, bold | 13 pt |
| Section `h2` / `h3` | same, bold | 10 pt / 9 pt |
| Product wordmark | same, bold, white | 12 pt |
| System output: job logs, spooled files, SQL results, command strings, configuration files, message text, reference codes, 5250 screen, object and job names | "Courier New", Courier, monospace | 9 pt (5250 screen 10 pt, reference code on a strip 12 pt) |

No web fonts, no icon fonts. Nothing bold except headings, the current navigation item, the current tab / wizard step, and unanswered inquiry messages.

## Page frame

Header band → left navigation column (190 px, text tree, `[+]` / `[-]` glyphs, current item bold) (`[+]` and `[-]` open and close a group in place through `/nav?open=` / `?close=`; the groups a user opened stay open for the session, and arriving at a page opens its group) → content column → one-line footer `QS/400 V1R0M0 · Quartz Systems`. Content starts with a breadcrumb (8 pt grey), the `h1`, an optional one-line grey description, then the message area, then the body. `System: QSHQ400  User: QSECOFR  Sign off  Help` sits at the right of the header on every page; Sign off is one click from everywhere and returns to that product's own sign-on page. The frame is a `<table class="frame">`; it is usable with CSS off.

Sign-on pages carry the same header band (wordmark left, `System: QSHQ400` right) and footer, then a left-aligned "Sign on" heading, User and Password in one label column with 30-character fields, and the Sign on button alone on the next row. Nothing else.

## Tables

- `table.list` for every list: 100 % wide, 1 px borders, 2–4 px cell padding, header row grey, rows alternating white / light blue.
- Sortable columns are header links (`?sort=name`). Sorting reloads the page.
- First column is **Actions**: text links separated by ` | ` (`Hold | Release | End | Job log | Properties`), or a per-row checkbox plus one `<select>` of actions and **Go** when there are more than six. The `<select>` may use `<optgroup>` to group actions the way task menus did.
- Numbers right-aligned (`td.num`); names, job numbers, paths, reference codes and commands in Courier (`td.mono`, no wrapping).
- Filter row above the table: a few `<select>` / `<input>` fields and **Go** (plus **Refresh** when the data is live).
- Paging line below: `Page 1 of 12  < Previous  Next >  Rows per page: [25 ▾] Go  318 entries`. Default 25 rows; 25 rows fit in 768 px.
- Empty state is one italic grey row spanning the table: `No active jobs match the filter.`
- `table.props` for properties: right-aligned grey labels (180 px), values on the right, bold subheadings as full-width rows, **Apply / OK / Cancel** below. Multi-section objects use text tabs `General | Options | Security | Logs` above the table; each tab is its own page.
- `table.status` for the label/value blocks on home pages.
- The only "chart" is `table.bar`: a 120 px one-row table with a coloured cell of proportional width.

## Buttons and controls

Browser defaults, never restyled. Labels are the verbs behind the CL commands: OK, Cancel, Apply, Refresh, Go, Start, Stop, Restart, End, Hold, Release, Delete, Remove, Add, Add..., Change, Browse..., < Back, Next >, Finish, Create, Run, Prompt, Reply, Send, Load, Apply, Vary on, Vary off, Download, Upload, Import, Assign, Activate, Shut down, Power on, Power off, Deploy. An ellipsis means "opens another page". Required fields carry a red `*`; complex fields carry a small `?` link that opens help in a new window.

## Messages

- One row, full width, directly under the title: yellow for information, green-tinted for completion, red-tinted for errors.
- Text is `MSGID - message text`, the ID in Courier and always a link to its second-level help (`CPF1321 - Job 123456/QUSER/QPADEV0001 ended.`).
- Message IDs are links everywhere they appear (lists, job logs, SQL messages, history log). Prefixes: CPF / CPA / CPI / CPC (system), SQL (database), HTP (HTTP server), TCP (network), QMC (management console).
- The error page is Message → Cause → Recovery, in that order, then technical details.
- Confirmations are separate pages (`Confirm End Job`, `Confirm Delete Instance`, `Confirm Power Off`): a sentence, a table of what is affected, options, **OK / Cancel**.

## Wizards

Left column lists the numbered steps (done = black, current = bold, pending = grey), content on the right, `< Back  Next >  Cancel` at the bottom, `Finish` (or the real verb, `Create`, `Deploy`) on the last step, which first shows every choice in a two-column table.

## QS/MC Management Console (addition)

QS/MC reuses the frame and all four patterns unchanged and adds **no lines to `shared/qs400.css`**. Things built from existing pieces that are worth naming:

- **Header band**: wordmark `QS/MC Management Console`; the right side reads `Console: QSHQMC1  User: qsadmin  Sign off  Help` (the label changes, the markup does not). Footer `QS/MC V1R0M0 · Quartz Systems`. The sign-on page carries a one-line notice `<p>` above the form.
- **Task-menu action select**: the per-list `<select>` + **Go** uses `<optgroup label="Operations">`, `Configuration`, `Connections`, `Serviceability`. Browser default rendering.
- **Control panel strip**: a one-row `table.list` above the terminal grid and on the Control panel functions page: Reference code (Courier, 12 pt, always a link), Meaning, then the buttons `03 IPL`, `08 Power off`, `21 Force DST`, `22 Dump`, `34 Retry`, a function-number field and **Select**.
- **Reference codes** (`C6004031`, `A6005008`, `B6000100`, `00000000`) are Courier and links on every row, never hidden behind Properties.
- **Verbs**: Activate / Shut down / Restart for partitions; Power on / Power off / Restart for managed systems; Load / Unload for optical media; Deploy for plans. Power off, Delete partition, Deploy, Install update and console shutdown always go through the confirmation page.

## A product outside QS/400

`net/qsweb` serves any family of products, not only QS/400's three; QS/MC builds its web UI on it (handoff 22). It depends on nothing of QS/400 (`tools/buildkit/tests/neutral.rs` holds it to that) and takes from the caller what is the caller's.

**What the family supplies** (`qsweb::brand::Brand`, one `static` for all its products):

| Field | QS/400's | What it is |
|---|---|---|
| `name` | `QS/400` | the shared pages' wordmark and title suffix (error, Not authorized), the help window's `QS/400 Help`, the footer's and the sign-on notice's first word |
| `vendor` | `Quartz Systems` | the footer's last words, the sign-on page's `(C)` line |
| `release` | `release::RELEASE` | anything `Display`: the footer, the sign-on line, `Server` |
| `server` | `QS400` | the `Server` header is `server/release` (`QS400/V1R2M0`) |
| `css_path`, `nav_path` | `/qs400/shared/qs400.css`, `/qs400/shared/qsnav.js` | where the product serves `qsweb::CSS` and `qsweb::NAV_JS` |
| `cookie` | `QS400SID` | the session cookie's name; its path is each product's prefix |
| `signon_notice` | `Unauthorized use ... audit journal.` | the line under the sign-on page's `(C)` line; empty, the `(C)` line stands alone with no `<br>` (QS/MC: `(C) Quartz Systems. QS/MC V1R0M0.`) |
| `notice_in_form` | `false` | where a product's sign-on notice goes: `false` a `<p>` above the form; `true` the sign-on table's first row, `<tr><td colspan="2"><p class="small">...</p></td></tr>` (QS/MC) |
| `user_max` | `10` | the sign-on page's User field's `maxlength` (QS/MC: 32) |
| `notauth_note` | `Contact the security officer (QSECOFR) ...` | the Not authorized page's last paragraph (HTML) |
| `job_log` | `true` | whether the error page offers `Job log` |

**What each product supplies** (`qsweb::frame::Product`): its family, its wordmark (`QS/MC Management Console`), the breadcrumb's root, its route prefix, its navigation tree, the label on the header's right (`System`, `Console`) and whether pages carry the navigation script. Per request (`Ctx`), the value after that label (the system's or the console's name), the user and the CSRF token. Routes, message texts (`qsweb::msg::MessageText`) and help texts are the product's own; nothing in `qsweb` reads a message file. A message ID of any prefix (`CPF2204`, `HTP8001`, `QMC2001`) renders the same: Courier, a link to the product's `/msg/ID`.

**What it may not change**: the stylesheet (served byte for byte; its file name and its first comment stay as they are), the frame's structure (`templates/frame.html`: header band, navigation column, content, footer), the sign-on page's form, and the patterns above (lists, properties, wizards, confirmation pages, message rows). A product adds pages built from `qsweb::partials`, never a stylesheet of its own.

**What the HTTP layer gives it.** `qsweb::http::serve` (and `serve_guarded`, a thread per request so a panic answers one request with an error page) runs on any `Read + Write`, so the product brings its own TLS (rustls' `StreamOwned` over a socket) and sets the socket's timeouts, which are the only time limits. The layer enforces a head (request line and headers) of 16 KiB (431), a body read whole of 8 MiB (413), 200 requests a connection, HTTP/1.0 and 1.1. A large upload is streamed instead of read whole: the handler says so in `Handler::streams` and reads the body as it arrives in `Handler::handle_stream` (`http::BodyStream`, chunked or by length, no size limit of the layer's; `http::multipart::Parts` reads a form's parts one at a time); a body left unread closes the connection. A large download is `Body::Stream` (chunked) or `Body::Sized` (with its length), read a piece at a time. `Handler::server` gives the `Server` header.

**Golden pages from another repository.** `qsweb::golden::diff` compares a served page with a prototype file after reducing both to tokens (whitespace, entity forms, attribute order and the CSRF field don't count); `Normalize` takes the caller's map from a served URL to the prototype's file and the navigation script's path. `golden::crawl` is the link check: from the start pages, every `GET` link fetched through the caller's function, redirects followed, links outside the product listed.

**The worked example** is the fixture (`tools/buildkit/fixture/init/src/web.rs`): the family `QSDEMO` (its own release, `/demo/shared/site.css`, cookie `QSDEMOSID`), a product `QSDEMO Management Console` with two navigation groups and `Console:` on the header's right, a sign-on, a settings form with its CSRF token and a `QMC2001` message row after it is posted. A P-256 certificate made by `x509kit::server::self_signed` serves it over TLS 1.3 on a socket pair; a client pinned to that certificate signs on, has a forged form refused, posts the real one and checks the page against the fixture's own golden file (`init/golden/settings.html`). `build fixture` runs it in the fixture's guest.

**A terminal window** (handoff 23). A screen of 5250 cells, or a partition's console, is drawn by `net/termlook`, which names no product and takes `qsipart` alone of this repository: the console's 9x16 font and palette, its cell rules, integer scaling, the status row below the grid (`X SYSTEM`, `INS`, `TLS`, the system, `RR/CCC`), greyed when the session is off; `screen.png` and Print Screen's PDF drawn on the server for a browser without script (`termlook::screen::Screen::png`, `print_screen` with the product's name as the PDF's producer). Fed from a 5250 screen (`screen::s5250`, Access for Web) or from a `qsipart::console::Grid` (`screen::grid`, QS/MC), cell for cell as the framebuffer draws it; `docs/ACCESSWEB.md` ("The look for a product outside QS/400") maps each console attribute and says where the two cannot look the same. With script, the page's `<script>` is `termlook::script::text(keypad)`; its element names the socket (`data-socket`), the picture it replaces (`data-image`), the scale and scheme, and switches on what only a 5250 session has (`data-5250`, `data-keymap`). The exchange on the socket is documented in `termlook::exchange` and in `ACCESSWEB.md`: `screen`, `delta`, `status`, `closed`, `message` out, keys by name and typed text in; `Input::console_keys` turns them into `qsipart::console::Key`s. Without script the page posts its keypad, `k=` one of `exchange::keys::KEYPAD`'s labels. The fixture's terminal page (`init/src/term.rs`) is the worked example: its menu sent as console frames, received into a grid, served as `screen.png` and checked against `init/golden/terminal.png`, the keypad's F3 decoded as the console's F3.

## What not to do

- No modals, no pop-up layers, no inline editing, no spinners, no toasts. Confirmation is a page; feedback is a message row after a reload.
- JavaScript only where it smooths what already works without it: the 5250 session / terminal window page (WebSocket terminal; its menu bar and Scale switch in place so the session stays drawn), and `shared/qsnav.js` for every product (`Product::nav_script`: HTTP Server Administration, Navigator, Access for Web): links, `GET` and `POST` forms inside the product fetch the next page and swap the body in, Back and Forward included, downloads saved where they are; a page with a script of its own, or one loaded with a refresh, navigates as usual. Every page still works with script off. Auto-refresh is `<meta http-equiv="refresh">`. Sorting, paging and filtering are links and `GET` forms.
- No icons beyond text glyphs or a 16×16 GIF (page, folder, status dot, plus/minus). The Access for Web home page may use one 32×32 glyph per section link. No emoji, no SVG illustrations, no hero images, no logos.
- No cards, panels, shadows, rounded corners, gradients, flexbox, grid, CSS variables or media queries (one print rule only).
- No restyled buttons, no coloured buttons, no accent fills. `#00d992` appears once, as a rule.
- No proportional fonts for anything an operator reads by column position.
- No IBM names, wording, logos or reproduced screens. Products are QS/400, QS/VS, QS/MC, Navigator for QS/400, QS/400 Access for Web, QS/400 HTTP Server Administration, QS/MC Management Console; the company is Quartz Systems.
- No page that needs more than 1024 px: tables stretch to 100 % and get wider on a bigger screen, never narrower than their columns.
