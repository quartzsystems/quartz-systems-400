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

## Steps

The handoff's build order, one commit each. The crawler test's
`PENDING` list names the routes not built yet; each step removes its
own entries.
