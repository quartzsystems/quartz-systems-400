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
  in user index `QZXWPRF`, made with the product and owned by
  `QTMHHTTP` (see The live server).
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
owned by `QTMHHTTP` (variable entries, a 30-byte key: the profile in
10, the function or preference key in 20; the value after it). The job
reads and writes them as itself; everything else runs as the
signed-on user, the job swapped to the profile handle taken at
sign-on (`work/qsjob/src/work/httpafw/`, over `web/` as Navigator).
`DLTLICPGM` removes the objects (`QUSRSYS` stays), `RSTLICPGM` brings
them back.

| Part | Live |
|---|---|
| System | `web::system::Core` (name, special authorities, class, groups, audit `PW`/`AF`, message texts: `XWA`/`QAD` from `QUSRSYS/QZXWMSG`); sign-on with the 5250 rules (`sign_on`; an expired password signs on to Change password only, no handle), `QSYGETPH`; `QSYCHGPW` (`sec_api ChangePassword`); `QPWDMINLEN`, `QPWDMAXLEN`, `QPWDRQDDIF`, `QPWDLVL`; the zone from `QTIMZON` (`sysval::tz`); the profile's text and the profiles (`QSYRUSRI`, `QSYLAUTU`, a group as Navigator decides); the counts: `msgq_list QUSRSYS/<user>`, `splf_list <user>`, the user's active jobs |
| Policy, Prefs | `usridx_find` (range of the profile's keys) and `usridx_add` on the two stores, as the job; the schemas Preferences offers: the libraries the user may list, `QGPL` and those without a `Q` name |
| The areas | not built: `httpafw/areas.rs` (`NotYet`); their routes answer `XWA0099` |

- `BLOCKED(rel)`: the password approval program (`QPWDVLDPGM`) is not
  called from the web job: Change password is refused with `CPF22D3`
  while one is set (`httpafw/system.rs`).

QEMU step 153 (`tools/conformance/src/afwstep.rs`) saves `QS00AFW`
(`SAVLICPGM`), deletes it (`STRTCPSVR ... HTTPSVR(ACCESSWEB)` then
answers `TCP1A06`), restores it (`RSTLICPGM`, the three objects back),
starts `HTTPSVR(ACCESSWEB)`, signs on as `QSECOFR` over 2003, checks My
home page against `DSPNETA`, changes and uses a preference, denies a
new profile's sign-on with a policy (`XWA0001`) and clears it, refuses
a POST without the form field, shows Database's `XWA0099`, signs off,
signs on and loads My home page over TLS on 2006 (the cookie
`Secure`), and ends the instance.

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
| 5-6b | 5250 session | not built (`XWA0099`) |
| 7-12 | Database | not built (`XWA0099`) |
| 13-14 | Files | not built (`XWA0099`) |
| 15-17 | Print, messages, jobs | not built (`XWA0099`) |
| 18-21 | Commands, download | not built (`XWA0099`) |
