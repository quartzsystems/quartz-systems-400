# Implementation handoff: QS/400 HTTP Server Administration (port 2001)

For Claude Code. This zip is the approved design for one product: the `ADMIN` instance of the QS/400 HTTP server, the web interface that creates and manages HTTP server instances and the system's certificates. Make a real server produce these pages from live system state.

## What is in the zip

```
qs400-http-server-administration/
  index.html              redirects to http/signon.html
  shared/qs400.css        the ONLY stylesheet. Ship it byte-for-byte.
  shared/components.html  every pattern once: frame, message area, list, properties, wizard, controls
  shared/confirm.html  shared/error.html  shared/help.html  shared/notauth.html   the generic pages, framed for this product
  http/                   26 pages of this product (golden renderings)
  STYLE.md                colours, type, table rules, verbs, message rules, what-not-to-do
  HANDOFF.md              this file
```

Pages of other products that this product links to directly are included so the prototype clicks through; they are reference only and are implemented by their own product:

- `nav/active-jobs.html` (from Navigator)
- `nav/end-job-confirm.html` (from Navigator)
- `nav/job-props-general.html` (from Navigator)

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

The HTTP server itself is a prerequisite: the admin product is the first instance the server crate must be able to run, so the server crate (HTTP/1.1, TLS, multipart, cookies, configuration file parser) comes before any admin page. Instances live under `/www/<name>/` (`conf/httpd.conf`, `htdocs/`, `logs/`); an instance registry (name, description, autostart flag, hot-backup partner) sits beside them. Each running instance is a job named after the instance in subsystem `QHTTPSVR`, user `QTMHHTTP`.

Navigation tree: Manage → Server instances; Create → New HTTP server; Certificates → Certificate stores, Applications; Advanced → Global settings. Route prefix `/qs400/httpadmin/`.

## Build order

One step = one commit, ending with the page matching its golden file.

| Step | Golden page(s) | Route(s) | Backend |
|---|---|---|---|
| 1 | `http/signon.html` | `GET/POST /signon` | user profile auth, session; sign-on requires `*IOSYSCFG` or membership of the administrators group set in Global settings |
| 2 | `http/instances.html` | `GET /instances?status=&name=&sort=&page=&rows=` | enumerate the registry; status and qualified job name from the `QHTTPSVR` job table; ports from `Listen` lines; the Job cell links to Navigator's job properties |
| 3 | `http/confirm-stop.html` | `GET /instance/<n>/confirm?op=start\|stop\|restart` → `POST` | `STRTCPSVR SERVER(*HTTP) HTTPSVR(<n>)` / `ENDTCPSVR … OPTION(*CNTRLD) DELAY(n)` or `*IMMED`; completion `HTP8001` / `HTP8002` into the message area of the instances list |
| 4 | `http/create-1.html` … `create-6.html` | `GET/POST /create/step/1..6` | wizard state in the session; step 6 creates `/www/<n>/{conf,htdocs,logs}`, writes `httpd.conf`, optional sample `index.html`, registry entry; message `HTP8020` |
| 5 | `http/instance-general.html` | `GET/POST /instance/<n>/general` | `ServerRoot`, `DocumentRoot`, `ServerUserID`, `ThreadsPerChild`; autostart and hot backup in the registry; Start button goes to step 3's confirmation |
| 6 | `http/instance-ports.html` | `/instance/<n>/ports` (+ `/add`, `/remove` POSTs) | `Listen` directives; TLS flag = `<VirtualHost *:port>` with `SSLEngine On`; ports < 1024 require `*IOSYSCFG` on the server user profile |
| 7 | `http/instance-directories.html`, `http/instance-directory-props.html` | `/instance/<n>/directories`, `/instance/<n>/directory?path=` | `<Directory>`, `Alias`, `ScriptAlias`; `Options`, `DirectoryIndex`, `AllowOverride`, `Order`/`Allow`/`Deny`, `AuthType`/`AuthName`/`PasswdFile` (validation list)/`Require` |
| 8 | `http/instance-tls.html` | `/instance/<n>/tls` | `SSLEngine`, `SSLAppName` (`QS400_HTTP_SERVER_<n>`), `SSLProtocolDisable`, cipher spec (`*DFT` = `QSSLCSL`), client authentication |
| 9 | `http/instance-logging.html` | `/instance/<n>/logging` | `CustomLog`, `LogFormat`, `ErrorLog`, `LogLevel`, `LogMaint` (rotation and keep), log CCSID |
| 10 | `http/instance-config.html` | `/instance/<n>/config` (Validate / Apply / Reset) | raw file in a 40-row textarea; Validate parses without writing and reports `HTP8100` with line and column; Apply writes atomically (temp + rename), keeps one `.bak`, marks the instance "restart required" |
| 11 | `http/instance-logs.html` | `/instance/<n>/logs?which=access\|error&n=200&q=` | tail N lines with an optional substring filter; `Download complete file` streams the log |
| 12 | `http/cert-stores.html`, `http/cert-store-system.html` | `/certs/stores`, `/certs/store/<name>` (+ create store, change password) | key database files under `/QS400/USERDATA/ICSS/CERT/`: `*SYSTEM`, `*OBJECTSIGNING`, `*SIGNATUREVERIFICATION`, user stores; tabs Server certificates / Certificate authorities / Certificate requests; expiry within 60 days renders in red |
| 13 | `http/cert-create-1.html` … `-3.html`, `http/cert-create-result.html` | `/certs/create/step/1..3`, `/certs/create/result` | RSA 2048/3072/4096 or ECDSA P-256/P-384; self-signed, local CA, or CSR; result page shows PEM, fingerprint, Download, Download with chain; message `HTP8301` |
| 14 | `http/cert-import.html` | `/certs/import` (multipart) | PEM / DER / PKCS#12 with password, or a path on the IFS; label defaults to CN; server or CA import; optional make default |
| 15 | `http/cert-apps.html` | `/certs/apps` (+ assign/remove POSTs) | application ID → certificate map read by every TLS-capable server: `QS400_HTTP_SERVER_ADMIN`, `QS400_HTTP_SERVER_<instance>`, `QS400_TELNET_SERVER`, `QS400_FTP_SERVER`, `QS400_DDM_DRDA_SERVER`, `QS400_CENTRAL_SERVER`, `QS400_UPDATE_CLIENT` |
| 16 | `http/global.html` | `/global` | defaults for new instances (user profile, server root, threads), max instances and threads, admin TLS redirect 2001→2010, session timeout, allowed admin networks, audit flag (QAUDJRN ZC) |
| 17 | `shared/confirm.html` | `/instance/<n>/confirm?op=delete` | refuse while running (`HTP8103`); otherwise remove `/www/<n>` and the registry entry; `HTP8021` |

Messages this product owns (message file `QHTTPSVR/QZHBMSG`, first- and second-level text): `HTP8000` instance starting, `HTP8001` started, `HTP8002` ended, `HTP8010` certificate loaded, `HTP8020` instance created, `HTP8021` instance deleted, `HTP8100` configuration error at line N column M, `HTP8103` instance must be stopped first, `HTP8110` request not authorized, `HTP8201` server user profile not found or not authorized, `HTP8301` certificate created, `HTP8302` certificate imported, `HTP8310` certificate assigned to application.

## Fixture data

Load the prototype's sample data as fixtures so the golden diffs are meaningful: instances `ADMIN` (2001), `APACHEDFT` (80), `WEBSTORE` (80, 443, stopped), `INTRANET` (8080, 8443), `API` (9443), `NAVIGATOR` (2002, 2005), `ACCESSWEB` (2003, 2006), `TESTSRV` (8000, stopped); six certificates in `*SYSTEM` including one expiring within 60 days and two expired; eleven certificate applications. System name `QSHQ400`, user `QSECOFR`.

## Product-specific acceptance

- Start / Stop / Restart / Delete and Apply on the configuration file cannot be reached by GET and require the CSRF token.
- Stopping `ADMIN` itself is refused with a message rather than ending the session you are in.
- `instance-config` Apply never leaves a half-written file; a killed process mid-write leaves the previous file in place.
- Certificate store passwords are never written to a log.
