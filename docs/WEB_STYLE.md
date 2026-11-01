# QS/400 web interfaces — style sheet in one page

Applies to QS/400 HTTP Server Administration (port 2001), Navigator for QS/400 (2002 / 2005), QS/400 Access for Web, and the QS/MC Management Console. One stylesheet, `shared/qs400.css` (177 lines). The reference page `shared/components.html` shows every pattern once. The test for any new page: would it look at home in a 2006 browser on a 1024×768 CRT, with CSS turned off, with no script? If not, it is wrong.

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

Header band → left navigation column (190 px, text tree, `[+]` / `[-]` glyphs, current item bold) → content column → one-line footer `QS/400 V1R0M0 · Quartz Systems`. Content starts with a breadcrumb (8 pt grey), the `h1`, an optional one-line grey description, then the message area, then the body. `System: QSHQ400  User: QSECOFR  Sign off  Help` sits at the right of the header on every page; Sign off is one click from everywhere and returns to that product's own sign-on page. The frame is a `<table class="frame">`; it is usable with CSS off.

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

## What not to do

- No modals, no pop-up layers, no inline editing, no spinners, no toasts. Confirmation is a page; feedback is a message row after a reload.
- No JavaScript anywhere except the 5250 session / terminal window page (WebSocket terminal). Auto-refresh is `<meta http-equiv="refresh">`. Sorting, paging and filtering are links and `GET` forms.
- No icons beyond text glyphs or a 16×16 GIF (page, folder, status dot, plus/minus). The Access for Web home page may use one 32×32 glyph per section link. No emoji, no SVG illustrations, no hero images, no logos.
- No cards, panels, shadows, rounded corners, gradients, flexbox, grid, CSS variables or media queries (one print rule only).
- No restyled buttons, no coloured buttons, no accent fills. `#00d992` appears once, as a rule.
- No proportional fonts for anything an operator reads by column position.
- No IBM names, wording, logos or reproduced screens. Products are QS/400, QS/VS, QS/MC, Navigator for QS/400, QS/400 Access for Web, QS/400 HTTP Server Administration, QS/MC Management Console; the company is Quartz Systems.
- No page that needs more than 1024 px: tables stretch to 100 % and get wider on a bigger screen, never narrower than their columns.
