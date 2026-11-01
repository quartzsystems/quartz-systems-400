# HTTP Server Administration

The `ADMIN` instance of the QS/400 HTTP server on port 2001 (TLS 2010):
the web interface that creates and manages HTTP server instances and the
system's certificates. The approved design is the handoff
(`docs/HTTPADMIN_HANDOFF.md`), the style standard `docs/WEB_STYLE.md`,
and the golden pages under `net/httpadmin/tests/golden/`.

## Crates

| Crate | What |
|---|---|
| `net/qsweb` | Shared by every QS/400 web product: HTTP/1.1 with keep-alive, forms, multipart, responses; sessions, CSRF, one-shot messages; the frame (askama), the sign-on page and help window; the partials; the generic pages; the golden comparison. `assets/qs400.css` ships byte for byte. |
| `net/httpd` | The HTTP server's instances: `httpd.conf` (parse, check, edit, print; errors with line and column), `Files` (IFS paths; `MemFiles`, `DirFiles`), the registry `/www/instances.json`, creating and deleting instances. |
| `net/httpadmin` | The product: `Env` (the traits it runs over: `System`, `Jobs`, `Files`, `Certs`), routes by area, the `HTP` messages, Global settings, help topics, the fixture and the demo binary. |

`cargo run -p httpadmin --bin httpadmin-demo [PORT]` serves the product
over the fixture at `http://127.0.0.1:2001/qs400/httpadmin/` (QSECOFR,
password `QS400`).

## Decisions

- Certificate application IDs and store paths stay IBM-compatible
  (`QIBM_HTTP_SERVER_ADMIN`, `/QIBM/UserData/ICSS/Cert/`): programs name
  them through the GSKit and `SSL_` APIs. The golden pages' `QS400_*`
  IDs and `/QS400/USERDATA` paths are data differences. The "no IBM
  wording" rule covers names of products, features and text; the
  hygiene test (`tests/behavior.rs`) skips `QIBM` identifiers.
- `ADMIN` listens on 2001 and, with TLS, 2010 (2005 is Navigator's).
- Routes: `/qs400/httpadmin/` + the prototype's file name as a path
  (`instance-ports.html` is `/instance/<name>/ports`). The stylesheet is
  `/qs400/shared/qs400.css` for every product. Help is
  `/help/<key>`, second-level text `/msg/<ID>?d=data&d=...`.
- A POST without the session's token gets 403. Every POST answers with
  a redirect to a GET page; the completion message rides in the session
  (`Rq::done`).
- Instance pages edit `httpd.conf` through `routes::instance::edit`:
  parse, change, write with `httpd::instance::apply_conf` (temporary
  file and rename, one `.bak`, restart required while running). Every
  change needs `*IOSYSCFG`; `ADMIN` cannot stop or restart itself
  (`HTP8104`). The Configuration file page keeps text being edited in
  the session until Apply or Reset.
- Directories & aliases: a `<Directory>` under the document root is
  reached below it, one elsewhere through an `Alias` naming it (one
  row); an alias's own options and access live in a `<Location>` for
  its URL; the `<Directory />` guard is not listed.
- `LogMaint path files size-MB [HH:MM]`: size 0 rotates daily at the
  time given (00:00 when absent). The registry's `hot_backup_active`
  switches the remembered hot backup partner on.
- Authority: `Rq::require(Need, function, object, crumb)` renders Not
  authorized and writes the `AF` entry. Sign-on needs `*IOSYSCFG`
  (`*ALLOBJ` implies it) or the administrators group of Global settings.

## Golden comparison

`tests/common` drives the handler with a browser in memory over the
fixture. `assert_golden(served, file, fixes)` maps served URLs to the
prototype's files (`proto`), drops the CSRF field, ignores whitespace
between tags and compares everything else. Fixes are edits applied to
the golden text before comparing; each is a real defect in the golden
page and is listed here:

| Page | Fix | Why |
|---|---|---|
| `http/signon.html` | form `action` `instances.html` → `signon.html` | the form posts to the sign-on route; the list follows by redirect |
| `shared/components.html` | tab links `components.html` → `help.html`; `error.html` → `help.html` | the pattern page links to itself; served, they are help windows |
| `http/confirm-stop.html` | `<form method="post">` moved before the How to end paragraph | the radios and delay field sat outside the form, so the choice was never submitted |
| `http/instance-ports.html` | `<form method="post">` moved before the list | the Select boxes sat outside the form, so Remove never saw them |
| `http/instance-tls.html` | `QS400_HTTP_SERVER_WEBSTORE` → `QIBM_HTTP_SERVER_WEBSTORE` | application IDs stay the compatible ones (Decisions) |
| `http/instance-config.html` | `QS400_HTTP_SERVER_WEBSTORE` → `QIBM_HTTP_SERVER_WEBSTORE`; `47 lines` → `45 lines` | compatible application IDs; the file the page itself shows has 45 lines |
| `http/instance-logs.html` | Download link `#` → the logs page's download route | the prototype's link goes nowhere; served, it streams the log |
| `http/cert-store-system.html` | `Set default` gets `formaction="cert-store-system.html"` | the button posted to Create certificate with nothing to tell it apart; it opens the store's default-certificate page |
| `http/cert-create-result.html` | `Download`, `Download with chain`, `Assign to application...` get names `download`, `chain`, `assign` | three nameless buttons in one form cannot be told apart |

## Steps

The handoff's build order, one commit each. The crawler test's
`PENDING` list names the routes not built yet; each step removes its
own entries.

## Certificates

The certificate pages run over `env::Certs`, which the fixture
implements with the real `net/certs` code (`certs::admin` over
`certs::files::MemFiles`, a test sealer, cheap Argon2 costs; the stores
are built once per test process). Every certificate page needs both
`*ALLOBJ` and `*SECADM` (`Rq::require_each`), as the `QS*` commands do.
Stores open through their stash; a store password typed on a page is
passed to the one call that needs it and is never put in the session, a
message, the audit journal or a page (`tests/certs.rs` checks).

Data differences the golden tests apply (`tests/golden.rs`): the store
paths (`/QIBM/UserData/ICSS/Cert/...`; `*SIGNATUREVERIFICATION` is
`Signing/SIGNVER.KDB`); on the create result, the serial number,
validity times, fingerprint and PEM of the certificate just made; on
Applications, the application IDs (`QIBM_QTV_TELNET_SERVER` for
`QS400_TELNET_SERVER`, `QIBM_QTMF_FTP_SERVER`, `QIBM_QRW_SERVER_DDM_DRDA`,
`QIBM_OS400_QZBS_SVR_CENTRAL`, `QIBM_HTTP_SERVER_<instance>`), sorted in
the Application select. The update client has no shipped ID; the fixture
registers `QS400_UPDATE_CLIENT` as a user-defined one. The
store page lists the default certificate first, then entries in the
order they were added.

## Global settings

`/global` edits `crate::settings::Settings` (`*IOSYSCFG`). A new session
timeout goes to `Admin::sessions` at once; the allowed networks must
include the address of the browser making the change; a change writes a
`ZC` entry when auditing was on before or after it (so turning auditing
off is recorded). The golden page's `QS400_HTTP_SERVER_ADMIN` is served
as `QIBM_HTTP_SERVER_ADMIN`.

## Serving instances

The server itself is `net/httpd/src/serve/`: `site::Plan` compiles an
`httpd.conf` (listeners and their TLS, the main server and each
`<VirtualHost>` as a `Site`), `Server` answers requests over `Deps`
(`Files`, an `auth::Authenticator`, a clock) and runs connections.
What it does: static files from `DocumentRoot` with media types
(`AddType`, `DefaultType`), `DirectoryIndex`, `Options +/-Indexes`
listings, `If-Modified-Since` (304), one byte range (206/416), `HEAD`,
`OPTIONS`; `Alias`; `ScriptAlias` answers 501 and logs it until CGI
exists (`TODO(later)`); `<Directory>` (shortest first), `<Files>`,
`<Location>`, `<Limit>`/`<LimitExcept>`; `Order`/`Allow`/`Deny` by
client address (a section naming any of them replaces the access group
before it); `AuthType Basic`, `AuthName`, `PasswdFile` (`%%SYSTEM%%`,
the default, is the user profiles; a validation list is refused until
`*VLDL` exists), `Require valid-user` / `user X`; `ErrorDocument` (a
local URL keeps the status, a URL redirects, text is the body);
`Redirect` (`permanent`, `temp`, `seeother`, `gone`, a number);
`CustomLog`/`LogFormat` (`%h %a %A %l %u %t %r %s %>s %b %B %{X}i
%{X}o %D %T %U %q %m %H %v %p %f`), `ErrorLog` (or `off`) with
`LogLevel`; `LogMaint path files size-MB [HH:MM]` (size 0 rotates
daily at the time, UTC, else at that size; rotated files are
`<name>.YYYYMMDDhhmmss`, the newest `files` kept); `KeepAlive`,
`MaxKeepAliveRequests`, `KeepAliveTimeout`, `TimeOut`; virtual hosts
by the connection's address and port, then `ServerName`/`ServerAlias`
against `Host`; TLS per listener from the virtual host on its port (or
the main server) with `SSLEngine On`, `SSLAppName` (default
`QIBM_HTTP_SERVER_<instance>`), `SSLProtocolDisable`, `SSLClientAuth`
(else the application's own setting). `Server::stop` is a controlled
end (the request in hand finishes, idle connections close),
`Server::drain` waits for them.

`httpd::bridge` carries `Files` operations from connection threads to
the one thread that owns the job's link (`Pump::serve`), and
`Pump::run` runs work that wants `Files` on a helper thread while the
owner answers. In `work/qsjob`, `work/ifsfiles.rs` is the IFS as a
`bridge::Local` and `with_files(link, |files| ...)` gives any job
`Files` over its own IFS view; the administration job can use both.

### The instance job and the commands

`STRTCPSVR SERVER(*HTTP) HTTPSVR(name)` starts job `name` in
`QHTTPSVR` under the configuration's `ServerUserID` (default
`QTMHHTTP`), role `HTTPSVR` (`work/qsjob/src/work/httpsvr.rs`);
`HTTPSVR(*ADMIN)` (or `ADMIN`) is still the administration instance.
`HTTPSVR(*ALL)` starts the administration instance and every registry
instance with `autostart`; `STRTCP` starts every registry instance
with `autostart` (and the administration instance when `*HTTP`
autostarts). An unknown name is `TCP1A06`, a running one `TCP1A08`
(refusals); started is `TCP1A07 HTTP server starting: instance X.`.
`ENDTCPSVR SERVER(*HTTP) HTTPSVR(name) OPTION(*CNTRLD|*IMMED)
DELAY(n)` (default `*CNTRLD`, 30 seconds) ends one; `HTTPSVR(*ALL)`
ends every running one; not running is `TCP1A09`. The `DELAY` reaches
the job through the registry entry's `end_delay`.

The job reads `/www/instances.json` and the instance's `httpd.conf`
through the IFS as its user, refuses ports below 1024 without
`*IOSYSCFG` (`HTP8201`), binds every `Listen` (a TLS port without a
certificate for its application ID is skipped and logged), writes
`last_started` (and clears `restart_required`), then `last_ended` and
`ended_how` (`ENDTCPSVR *HTTP, controlled` / `, immediate`, `ENDJOB,
...`, `ENDSBS, ...`) to the registry, and `HTP8000`, `HTP8001`
(`listening on *:80, *:443 (TLS)`), `HTP8100`, `HTP8002` to its job
log (program `QZHBHTTP`).

For the administration pages: an instance's job is the active job of
`QHTTPSVR` named after it (`active_jobs()` gives number, user and
start time); its **function** is `CNN-n`, the client connections open
now, refreshed every half second.

## The live administration server

`ADMIN` (role `HTTPADM`, `work/qsjob/src/work/httpadm/`) serves the
product on 2001 and, with TLS, on 2010 (Global settings' TLS port) through
`servers::web::Web`, kept alive. The job's thread holds the link and the
runtime; connection threads hand it closures (`bridge.rs`), so service
requests never interleave. The environment:

- **System**: the name is `SYSNAME` of the network attributes, else
  `QSYSNAME`; dates in the job's `DATFMT`/`DATSEP`/`TIMSEP` and the
  `QTIMZON` zone (`httpenv/stamp.rs`). Sign-on is the service's
  `sign_on` with no device (`QMAXSIGN`, `PW` `U`/`P`/`Q` entries); an
  expired password is refused (`CPF22E4`, a `PW` `R` entry); then the
  profile's handle (`QSYGETPH`) is kept for the user. Special
  authorities, groups and class are read with the job swapped to the
  user (`whoami`, the profile), kept 30 seconds. Audit entries go
  through `web_audit` (only a `QTMHHTTP` job may send it). Message
  texts come from `QHTTPSVR/QZHBMSG` (`HTP`), `QTCPMSG` (`TCP`) or
  `QCPFMSG`.
- **Jobs**: the active jobs of `QHTTPSVR` but its monitor, one per
  name; connections from the function `CNN-n`. Start and end run
  `STRTCPSVR`/`ENDTCPSVR ... HTTPSVR(name)` through the analyzer with
  the job swapped to the web user (`as_user`), so the commands check
  their own authority; the escape (or the diagnostic before `CPF0001`)
  is the error message.
- **Files**: `work/ifsfiles.rs` over the job's link, as `QTMHHTTP`.
- **Certs**: `httpadmin::certmgr` (shared with the fixture) runs in the
  storage service (`web_certs`, `lic/slic/src/stgsvc/webadm.rs`) over the
  store files `certs.rs` stages; the job sends `certmgr::wire` calls,
  those that change stores as the web user, whose `*ALLOBJ` and
  `*SECADM` the service checks. The stores someone made are listed in
  `certs::paths::USER_STORES`. An import from a path is read in the job
  as the user.

`QZHBMSG` is made from `src/msgs.rs`: after adding a message run
`QS400_UPDATE_MSGF=1 cargo test -p httpadmin --test msgf`. The HTTP
server is licensed program `QS00DG1` (`5770DG1`, docs/LICPGM.md),
installed on every new system: its `*BASE` code load is library
`QHTTPSVR` with `QZHBMSG`, made by `lic/osobjects/src/products.rs` when
the service seeds the catalog at IPL and by `build lp` for the
product's load, so `CHKPRDOPT PRDID(QS00DG1)` checks the file. On a new
system the job makes instance `ADMIN` (`/www/admin`, `seed.rs`). QEMU
step 87 (`tools/conformance/src/httpadm.rs`) signs on as `QSECOFR` over
2001, checks Server instances against `DSPJOB`, a message page, the
CSRF refusal and sign-off, and the sign-on page over TLS on 2010.

## Server user profiles

Everything the administration server makes under `/www` is owned by
`QTMHHTTP`, the `ADMIN` job's user. An instance whose `ServerUserID` is
another profile is given, when it is created or its profile changed,
`*RWX` on `/www` (to replace the registry), `*RX` on its server root,
`*RWX` on its `logs` and `*RW` on `/www/instances.json`
(`env::authorize_commands`, run by the job as the files' owner).

"Require TLS" redirects 2001 to the TLS port only while that port
listens (`Admin::tls_up`), and cannot be switched on while it does not,
so a TLS port that fails to start never locks administration out.
