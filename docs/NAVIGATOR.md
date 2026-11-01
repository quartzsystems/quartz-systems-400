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

## Golden comparison

`tests/common` drives the handler with a browser in memory over the
fixture; `proto()` maps every served URL to its prototype file.
`assert_golden(served, file, fixes)` drops the CSRF field, ignores
whitespace between tags and compares the rest. Fixes are real defects
of the golden page:

| Page | Fix | Why |
|---|---|---|
| `nav/signon.html` | form `action` `home.html` → `signon.html` | the form posts to the sign-on route; home follows by redirect |

## Steps

The handoff's build order, one commit each; `tests/behavior.rs`'s
`PENDING` names the routes not built yet and each step removes its own.

| Steps | Area | State |
|---|---|---|
| 1, 2 | Frame, sign-on, System status | done |
| 3-11 | Work management | |
| 12-14 | Messages | |
| 15-18 | Users and groups | |
| 19-22 | System | |
| 23, 24 | Network | |
| 25, 26 | Database | |
| 27-30 | Security, software, performance | |
