# Security

Phase 6 makes security real: user profiles and passwords, object
authority with private authorities, authorization lists, groups and
adopted authority, domain and state, security auditing and the
security system values. This document is the reference; `PHASE6.md`
says what was built and measured, `SEC_OPEN_QUESTIONS.md` lists the
readings a machine must settle.

## 1. User profiles

A profile is a `*USRPRF` object in `QSYS`. Its space holds a four-byte
length and the payload: the fixed part below, then a tail of tagged
items (a one-byte tag, a two-byte length, the bytes; tag 0 ends it):

| Tag | Item |
|---|---|
| 1 | a supplemental group profile, once each, at most fifteen |
| 2 | `LOCALE` |
| 3 | `HOMEDIR` |
| 4 | a `SETJOBATR` value, once each |
| 5 | a `USROPT` value, once each |
| 6 | a password hash of the current password: a form byte (0 short, 1 long) and the PHC string |
| 7 | a previous password's hash, the same way, newest first |
| 8 | an `AUDLVL` value, once each |

Three of the profile object's tree root slots hold its indexes, B+trees
keyed by object ID: slot 0 the objects it owns, slot 1 the objects it
has a private authority to (value: the bits), slot 2 the objects whose
primary group it is. The store keeps them as objects are created,
deleted, re-owned and authorized, so `DSPUSRPRF *OBJOWN`, `*OBJAUT`
and `DLTUSRPRF` never scan the system.

### `UserProfile` (512 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | Magic | `QSSECUSR` |
| 8 | 2 | Version | 1 |
| 10 | 1 | Status | 0 `*ENABLED`, 1 `*DISABLED` |
| 11 | 1 | UsrCls | 0 `*USER`, 1 `*SYSOPR`, 2 `*PGMR`, 3 `*SECADM`, 4 `*SECOFR` |
| 12 | 4 | SpcAut | bits: 0x01 `*ALLOBJ`, 0x02 `*SECADM`, 0x04 `*JOBCTL`, 0x08 `*SPLCTL`, 0x10 `*SAVSYS`, 0x20 `*SERVICE`, 0x40 `*AUDIT`, 0x80 `*IOSYSCFG` |
| 16 | 1 | PwdExp | 1 `PWDEXP(*YES)` |
| 17 | 1 | PwdNone | 1 `PASSWORD(*NONE)` |
| 18 | 1 | PwdForms | bit 0 short form kept, bit 1 long form kept |
| 19 | 1 | LmtCpb | 0 `*NO`, 1 `*PARTIAL`, 2 `*YES` |
| 20 | 1 | DspSgnInf | 0 `*SYSVAL`, 1 `*NO`, 2 `*YES` |
| 21 | 1 | LclPwdMgt | 1 `*YES` |
| 22 | 1 | LmtDevSsn | 0 `*SYSVAL`, 1 `*NO`, 2 `*YES` |
| 23 | 1 | PtyLmt | 0-9 |
| 24 | 4 | PwdExpItv | days; 0 `*SYSVAL`, 0xFFFFFFFF `*NOMAX` |
| 28 | 4 | PwdChgBlk | hours; 0 `*SYSVAL`, 0xFFFFFFFF `*NONE` |
| 32 | 8 | MaxStg | signed KB, -1 `*NOMAX` |
| 40 | 8 | MaxStgLrg | signed KB, -1 `*NOMAX` |
| 48 | 8 | StgUsed | KB the owned objects use |
| 56 | 1 | Owner | 0 `*USRPRF`, 1 `*GRPPRF` |
| 57 | 1 | GrpAutTyp | 0 `*PRIVATE`, 1 `*PGP` |
| 58 | 1 | Sev | 0-99 |
| 59 | 1 | ObjAud | 0 `*NONE`, 1 `*CHANGE`, 2 `*ALL` |
| 60 | 4 | Ccsid | 0 `*SYSVAL` |
| 64 | 4 | Uid |  |
| 68 | 4 | Gid | 0 `*NONE` |
| 72 | 4 | UsrExpItv | days, 0 none |
| 76 | 4 | SignOnInvalid | attempts since the last good sign-on |
| 80 | 8 | Created | Unix seconds |
| 88 | 8 | Changed | Unix seconds |
| 96 | 8 | LastSignOn | Unix seconds, 0 never |
| 104 | 8 | PrevSignOn | Unix seconds |
| 112 | 8 | PwdChanged | Unix seconds |
| 120 | 8 | UsrExpDate | Unix seconds at midnight UTC, 0 `*NONE` |
| 128 | 8 | OwnedCount | entries in the owned index |
| 136 | 8 | Reserved | 0 |
| 144 | 10 | Name | CP037, as are the text fields below |
| 154 | 10 | AstLvl |  |
| 164 | 10 | CurLib | or `*CRTDFT` |
| 174 | 10 | InlPgm | or `*NONE` |
| 184 | 10 | InlPgmLib |  |
| 194 | 10 | InlMnu | `MAIN`, `*SIGNOFF` ... |
| 204 | 10 | InlMnuLib |  |
| 214 | 10 | SpcEnv |  |
| 224 | 10 | KbdBuf |  |
| 234 | 10 | JobD |  |
| 244 | 10 | JobDLib |  |
| 254 | 10 | GrpPrf | or `*NONE` |
| 264 | 10 | GrpAut | `*NONE`, `*ALL`, `*CHANGE`, `*USE`, `*EXCLUDE` |
| 274 | 15 | AcgCde |  |
| 289 | 10 | Dlvry |  |
| 299 | 10 | MsgQ | `*USRPRF` or a name |
| 309 | 10 | MsgQLib |  |
| 319 | 10 | PrtDev |  |
| 329 | 10 | OutQ |  |
| 339 | 10 | OutQLib |  |
| 349 | 10 | AtnPgm |  |
| 359 | 10 | AtnPgmLib |  |
| 369 | 10 | SrtSeq |  |
| 379 | 10 | SrtSeqLib |  |
| 389 | 10 | LangId |  |
| 399 | 10 | CntryId |  |
| 409 | 10 | ChrIdCtl |  |
| 419 | 10 | EimAssoc | stored only |
| 429 | 10 | AuthMth | stored only |
| 439 | 10 | DocPwd | stored only |
| 449 | 50 | Text |  |
| 499 | 13 | Reserved2 | 0 |

`SPCAUT(*USRCLS)` is resolved when the profile is created or changed:
the class's defaults at the current `QSECURITY` are stored.

| Class | 10 and 20 | 30 and up |
|---|---|---|
| `*SECOFR` | all eight | all eight |
| `*SECADM` | `*ALLOBJ *SAVSYS *SECADM *JOBCTL` | `*SECADM` |
| `*PGMR` | `*ALLOBJ *SAVSYS *JOBCTL` | none |
| `*SYSOPR` | `*ALLOBJ *SAVSYS *JOBCTL` | `*SAVSYS *JOBCTL` |
| `*USER` | `*ALLOBJ *SAVSYS` | none |

### Shipped profiles

`sec::profile::shipped::PROFILES` holds every shipped profile with
IBM's name, class and special authorities. All have `PASSWORD(*NONE)`
except `QSECOFR`, whose password is `QSECOFR`, expired, so the first
sign-on must change it. They are owned by `QSYS` with `*PUBLIC
*EXCLUDE`.

The first IPL with security re-owns what already exists: the system
libraries (`QSYS`, `QGPL`, `QUSRSYS`, `QSPL`, `QRECOVERY` ...) and
everything in them go to `QSYS`, with IBM's public authority for the
type (`*USE` for most objects, `*CHANGE` for `QGPL` and message queues,
`*EXCLUDE` for profiles, the system value object, spooled files, `QSPL`
and `QRECOVERY`); a user's message queue in `QUSRSYS` goes to its user;
anything else whose owner is not a profile goes to `QDFTOWN`. Then the
profile indexes are built from the objects.

## 2. Passwords

`QPWDLVL` decides the form a password is accepted and checked in:

| Level | Accepted | Checked against | Kept |
|---|---|---|---|
| 0, 1 | 1-10 characters of `A-Z 0-9 $ @ # _`, not starting with a digit, folded to upper case | the short form | short, and the long form of the folded password |
| 2 | 1-128 characters, any, case-sensitive | the long form | long, and short when the password fits the short rules |
| 3 | as 2 | the long form | long only |

Each kept form is an Argon2id hash (`argon2` 0.5.3, pinned; its default
parameters: version 19, 19 MiB, two passes, one lane) of the password
as the form canonicalizes it, with sixteen salt bytes of its own from
`/dev/urandom`, stored as the PHC string. No IBM hash format is ever
stored or produced; `QSYRUPWD` returns these strings.

### Rules

When `QPWDRULES` is `*PWDSYSVAL` the classic values apply:
`QPWDMINLEN`, `QPWDMAXLEN`, `QPWDLMTAJC`, `QPWDLMTREP` (1 no character
twice, 2 not twice in a row), `QPWDPOSDIF`, `QPWDRQDDGT` and
`QPWDLMTCHR`. Otherwise those are ignored and the `QPWDRULES` tokens
rule: `*ALLCRTCHG`, `*CHRLMTAJC`, `*CHRLMTREP`, `*DGTLMTAJC`,
`*DGTLMTFST`, `*DGTLMTLST`, `*DGTMAXn`, `*DGTMINn`, `*LTRLMTAJC`,
`*LTRLMTFST`, `*LTRLMTLST`, `*LTRMAXn`, `*LTRMINn`, `*LMTSAMPOS`,
`*LMTPRFNAME`, `*LWRMAXn`, `*LWRMINn`, `*MAXLENn`, `*MINLENn`,
`*MIXCASEn`, `*REQANY3`, `*SPCCHRLMTAJC`, `*SPCCHRLMTFST`,
`*SPCCHRLMTLST`, `*SPCCHRMAXn`, `*SPCCHRMINn`, `*UPRMAXn`, `*UPRMINn`.
Levels 0 and 1 never take more than ten characters whatever the rules
say. The rules bind a user changing their own password; `CRTUSRPRF`
and `CHGUSRPRF PASSWORD` are bound only with `*ALLCRTCHG` (otherwise
only the level's form is checked).

A composition failure is `CPF22C2` (the identifier an IBM i 7.1 sends
for a `QPWD*` rule) with IBM's reason code:

| Reason | Rule |
|---|---|
| 1, 2, 3 | too few letters, digits, special characters |
| 4, 5, 6 | too many letters (or upper, or lower case), digits, special characters |
| 7, 8, 9 | starts with a letter, digit, special character |
| 10, 11, 12 | ends with a letter, digit, special character |
| 13, 14, 15 | two letters, digits, special characters side by side |
| 16 | too few upper or lower case letters (`*MIXCASE`, `*UPRMIN`, `*LWRMIN`) |
| 17 | fewer than three kinds (`*REQANY3`) |
| 18 | contains the profile name |
| 19, 20 | too short, too long |
| 21 | a character used twice |
| 22 | a character twice in a row |
| 23 | a character in the same position as in the old password |
| 24 | a `QPWDLMTCHR` character (Quartz's code; IBM's classic message is unsettled) |

The other refusals:

| Message | When |
|---|---|
| `CPF2362` | `CHGPWD`: the current password typed is wrong |
| `CPF22C3` | the new password is the current one, or one of the last `QPWDRQDDIF` passwords (1-8 keep 32, 24, 18, 12, 10, 8, 6, 4) |
| `CPF22C5` | `PWDCHGBLK`/`QPWDCHGBLK` hours have not passed since the last change (an expired password may always change) |
| `CPF22C6` | `QPWDVLDPGM` refused it |
| `CPF22E2` | an API's password is wrong for the profile |
| `CPF22E3` | the profile is disabled |
| `CPF22E4` | the password has expired |
| `CPF22E5` | the profile has `PASSWORD(*NONE)` |

History keeps the hashes of up to 64 previous forms, newest first; a
new password is compared with the current one and the last
`QPWDRQDDIF` count in the form the level checks.

A password expires when `PWDEXP(*YES)` is set, or when its age reaches
`PWDEXPITV` days (`*SYSVAL` is `QPWDEXPITV`; `*NOMAX` never). Within
`QPWDEXPWRN` days of that, sign-on warns.

`QPWDVLDPGM` names a program called through the ILE call path with
IBM's parameters: the new password, the old password, a one-character
return code (`0` accepts) and the profile name. At levels 0 and 1 the
passwords are `CHAR(10)`; at 2 and 3 each is a four-byte length, a
four-byte CCSID and the characters.

## 3. Authorities

An authority set is the header's 32-bit field (`STORE_FORMAT.md` §8):

| Bit | Authority | Kind |
|---|---|---|
| 0x0001 | `*OBJOPR` | object |
| 0x0002 | `*OBJMGT` | object |
| 0x0004 | `*OBJEXIST` | object |
| 0x0008 | `*OBJALTER` | object |
| 0x0010 | `*OBJREF` | object |
| 0x0400 | `*AUTLMGT` | object, authorization lists only |
| 0x0020 | `*READ` | data |
| 0x0040 | `*ADD` | data |
| 0x0080 | `*UPD` | data |
| 0x0100 | `*DLT` | data |
| 0x0200 | `*EXECUTE` | data |

The combinations expand to exactly these bits and display back as the
combination only when the bits match; any other set shows as `USER
DEF`:

| Combination | Bits |
|---|---|
| `*ALL` | every object and data authority (with `*AUTLMGT` on an authorization list) |
| `*CHANGE` | `*OBJOPR` and every data authority |
| `*USE` | `*OBJOPR *READ *EXECUTE` |
| `*EXCLUDE` | none; as a private authority it still decides |

An object carries:

- its owner and the owner's authority, `*ALL` unless reduced (IBM
  allows reducing it; `auth_flags` bit 0 records that it was);
- `*PUBLIC`, or `*PUBLIC` taken from the authorization list (bit
  0x8000_0000);
- its primary group and that group's authority;
- private authorities: sixteen in the header, the rest in the overflow
  tree, each also in the profile's private index;
- the authorization list securing it, by name;
- an authority generation, bumped by every change, which cached
  decisions compare.

`CHGOBJOWN` gives the new owner the owner's authority and drops its
private one; the old owner keeps `*ALL` privately unless
`CUROWNAUT(*REVOKE)`. The owner's storage moves with the object.
`CHGOBJPGP` turns a private authority the new group had into its
group authority; the old group keeps its authority privately unless
`RVKOLDAUT(*YES)`. An object's owner cannot be its primary group.

### Authorization lists

A `*AUTL` is an object in `QSYS` whose private authorities are the
list's entries and whose `*PUBLIC` is the list's public authority; its
entries therefore live where any object's private authorities do.
`*AUTLMGT` on the list lets a user add and remove entries. Tree root
slot 0 indexes the objects the list secures (`DSPAUTLOBJ`). A list
cannot secure another list.

### The commands

`sec::cmd::object` holds `GRTOBJAUT`, `RVKOBJAUT`, `DSPOBJAUT`
(`EDTOBJAUT` shows the same list on the serial console), `CHGOBJOWN`,
`CHGOBJPGP`, `CHKOBJ`, `GRTUSRAUT`, `WRKOBJOWN` and the list commands
`CRTAUTL`, `ADDAUTLE`, `CHGAUTLE`, `RMVAUTLE`, `DSPAUTL`,
`DSPAUTLOBJ`; `sec::cmd::dltusrprf` holds `DLTUSRPRF` and
`ANZDFTPWD`. Each checks through the guard, so a job's session
decides and a refusal is audited like any other:

- `GRTOBJAUT` and `RVKOBJAUT` need `*OBJMGT` (`obj.grant`); a user who
  is neither the owner nor `*ALLOBJ` may give only authorities it has
  itself (`CPF2227`). Single authorities (`*OBJMGT`, `*ADD` ...) add
  to what the user has; a combination (`*ALL`, `*CHANGE`, `*USE`,
  `*EXCLUDE`) or `REPLACE(*YES)` replaces it; a revoke that leaves
  nothing removes the private authority. `AUTL()` secures or releases the
  object, for the owner or `*ALLOBJ` only. `AUT(*AUTL)` gives
  `*PUBLIC` from the list.
- `DSPOBJAUT` shows every user to the owner and to `*OBJMGT`, and only
  the user's own row otherwise.
- `CHGOBJOWN` needs `*OBJEXIST` and `*ADD` to the new owner's profile;
  `CUROWNAUT` defaults to `*REVOKE`.
- `ADDAUTLE`, `CHGAUTLE` and `RMVAUTLE` need `*AUTLMGT` on the list,
  with the same rule on what a non-owner may give.
- `DLTUSRPRF` needs `*SECADM`; a shipped profile, one's own, or a group
  with members cannot be deleted. Owned objects stop the delete under
  `OWNOBJOPT(*NODLT)`, are deleted under `*DLT` or move under
  `*CHGOWN user`; objects it is the primary group of stop it under
  `PGPOPT(*NOCHG)` or get a new group (or `*NONE`) under `*CHGPGP`.
  Its private authorities go with it.
- `ANZDFTPWD` lists the profiles whose password is their name and, with
  `ACTION(*DISABLE *PWDEXP)`, disables them or expires the password.
- `DSPUSRPRF` of another user needs `*READ` and `*OBJOPR` to its
  profile (`usrprf.display`), as `QSYRUSRI` does.

## 4. The authority search

`sec::auth::search` is the one implementation; the store's guard, the
commands and `CHKOBJ` all call it. For a user `U` with groups `G1..Gn`
(`GRPPRF`, then `SUPGRPPRF` in order) and an object `O`:

| Step | Source | Rule |
|---|---|---|
| 1 | `*ALLOBJ` | `U` has it (or `QSECURITY` is 20 or less): allowed |
| 2 | owner | `U` owns `O`: the owner's authority decides |
| 2 | private | `U` has a private authority to `O`: it decides, even `*EXCLUDE`, even when `*PUBLIC` has more |
| 3 | list | `U` owns or has an entry on the list securing `O`: it decides |
| 4 | group `*ALLOBJ` | any group has it: allowed |
| 4 | groups | for each group: primary group authority if it is `O`'s primary group, else its owner, private or list entry; the entries found add up and decide together |
| 5 | `*PUBLIC` | only when neither `U` nor a group had an entry: `O`'s `*PUBLIC`, or the list's when `O` takes it from the list |
| 6 | adopted | when 1-5 fall short: each owner the call stack adopts, nearest first, adds its own authority (its `*ALLOBJ`, or its owner, private or list entry; not its groups) to what was found, until it suffices |

Required authorities are `need` sets from `security/sec/required.toml`,
keyed by operation and, where one type differs, `operation@TYPE`.
`*ANY` is any authority but `*EXCLUDE`. A check names the operation;
the guard looks up the need for the object's type. Library operations
(`lib.*`) are checked against the library before the object: naming a
library needs `*EXECUTE`, creating in it `*READ *ADD`, renaming in it
`*UPD *EXECUTE`, moving out `*DLT *EXECUTE` and in `*READ *ADD`. A
refusal on a library is `CPF9820`, on an object `CPF2189`, with the
object, library and type as IBM's substitution data. The step that
decided is recorded with every answer: the `AF` audit entry carries it,
and `DSPOBJAUT` will show it as the authority source.

### Where checks happen

The store calls the guard (`store::obj::guard`) in its own operations,
so every caller is checked the same way. The storage service installs
`sec::guard::SecGuard` on ASP 1 and runs each job request inside
`sec::guard::as_job` with the job's session; work the system does for
itself (starting a job, the job log, spooling into `QSPL`) runs inside
`as_system`. A store without a guard, or a request without a session,
is the system and is not checked: formatting, the IPL, tests. The
temporary ASP has no guard: `QTEMP` belongs to one job.

On creation the guard sets the owner (the user, or its group with
`OWNER(*GRPPRF)`), the primary group with `GRPAUTTYP(*PGP)`, `*PUBLIC`
from the library's `CRTAUT` (`*SYSVAL` is `QCRTAUT`; a list name
secures the object by that list with `*PUBLIC *AUTL`), the object
auditing value from `QCRTOBJAUD`, and gives `GRPAUT` privately to the
group with `GRPAUTTYP(*PRIVATE)`. It indexes the object under its
owner, and charges its storage to the owner as it grows (refused past
`MAXSTG`, `CPF1151`). Deletion takes the object out of every index.

A refusal reaches a program as an escape with the guard's identifier
(`CPF2189`, `CPF2207`, `CPF9802` ...), on a file open through the
database's feedback and on a data area or queue through the job's
source, so a program monitors it like any other condition.

`security/sec/exempt.toml` lists every public store, database,
journal and work management function that takes a transaction yet
checks nothing itself, with the reason (its caller checks, it runs as
the system, it is the store's own plumbing). `tests/exempt.rs` scans
the sources: an unchecked function missing from the list fails, and so
does an entry that has started checking or no longer exists. The same
test holds `Job::call_program` and `load_program` to their state check.

### The cache

A job's session (`sec::auth::cache::Session`) keeps, per object, what
steps 1 to 5 found, with the object's authority generation and the
system's security epoch. A grant, a revoke, a new owner, primary group
or list bumps the object's generation; a change to a profile, a group
or a list's entries bumps the epoch. Either makes the entry stale.
Every check runs in the storage service, one process, so the epoch is
a counter in memory and nothing is broadcast to the job processes.
Adopted authority is never cached: the call stack changes it.

## 5. Adopted authority

A program or service program created with `USRPRF(*OWNER)` adopts its
owner's authority while it is on the call stack. The job's activator
(`mi::act::Activator::adopting`) walks the stack from the newest entry
down: each `*OWNER` program adds its owner; a program with
`USEADPAUT(*NO)` keeps the adoption of the programs below it from
reaching those above. Control boundaries do not stop it. When the list
changes, and only while some program on the stack adopts, the job sends
it to the storage service (`adopt`, request 124); the service looks each
program up, ignores one that is not `USRPRF(*OWNER)`, and puts its
owner's authorities (its own special authorities and its owner,
private and list entries; not its groups) in the job's principal.
Adopted authority is added to what steps 1 to 5 found and is never
cached.

`USEADPAUT(*YES)` is the default. When `QUSEADPAUT` names an
authorization list, a program created by a user without `*USE` to the
list is made `USEADPAUT(*NO)`, and `CRTPGM` says so.

## 6. Security levels, domain and state

`QSECURITY` takes effect at an IPL: the storage service reads it once
as it starts and every job's principal and runtime use that level. The
level the last IPL ran at is kept in `QSYS/QSECLVL`; going from 20 or
below to 30 or above takes `*ALLOBJ` from every profile whose class is
not `*SECOFR`.

| Level | Sign-on | Authority | Domain and state |
|---|---|---|---|
| 20 | user and password | everyone has `*ALLOBJ` and `*SAVSYS` | not enforced |
| 30 | as 20 | the search of §4 | not enforced |
| 40 | as 20 | as 30 | enforced |
| 50 | as 20 | as 30 | enforced, and API parameters checked |

Objects carry a domain (`*SYSTEM`, `*USER`) and programs a state. At 40
and 50 the runtime (`mi`) refuses, with `MCH6801`:

- loading a user-state program that imports a procedure only system
  state may use (`ile::binder::PRIVILEGED_PROCS`: raw storage, direct
  storage service calls, authority bypass); at 30 the import resolves to
  nothing and a call is `MCH3401`;
- a call from a user-state program to a system-state program; the
  published entry points, the `sysapi` programs, are not programs but
  natives of the runtime and stay callable. Command level counts as
  system state (`QCMD`), so routing and the startup program call any
  program;
- `QUSPTRUS` from user state on a user space in the system domain.

At 50 a user-state caller of an API may pass only user-domain storage:
the runtime refuses a parameter that points into a system-state
program's static area. Changing an object's domain is not offered by
any API at any level.

A user space created in a library `QALWUSRDMN` does not list (`*ALL`
lists every one; `QTEMP` is always allowed) is in the system domain.
Programs carry the domain and state of their attributes in their
header. Only development images (`qs400.svc=1`) let `CRTPGM
STATE(*SYSTEM)` make a system-state program, for the harness.

Jobs never reach pages: every object operation is a typed request the
storage service performs, so there is no raw page access for a
user-state job to be refused.

## 7. Auditing

Audit entries are journal entries of code `T` in `QSYS/QAUDJRN`, the
Phase 3 journal. Neither the journal nor its first receiver is shipped:
`CHGSECAUD` creates `QSYS/AUDRCV0001` and `QSYS/QAUDJRN` when they are
missing, or create them yourself (`CRTJRNRCV QSYS/AUDRCV0001`, `CRTJRN
QSYS/QAUDJRN JRNRCV(QSYS/AUDRCV0001)`).

Code that does something auditable records an event
(`sec::audit::record`); the storage service takes a request's events
after it answers, keeps the ones the values select and writes them in a
transaction of their own, so a request that failed (an authority
failure) is audited too. An event is written when:

- `QAUDCTL` has `*AUDLVL` and its type's value (below) is in `QAUDLVL`
  (with `QAUDLVL2` when `QAUDLVL` lists `*AUDLVL2`) or in the user's
  `AUDLVL` (`CHGUSRAUD`); `*SECURITY` stands for every `*SECxxx` value
  and `*JOBDTA` for `*JOBBAS` and `*JOBCHGUSR`; or
- `QAUDCTL` has `*OBJAUD` and the object's `OBJAUD` asks for it:
  `*ALL` reads (`ZR`) and changes (`ZC`), `*CHANGE` changes, `*USRPRF`
  what the user's `OBJAUD` says. An access is a change when its
  operation needs more than `*OBJOPR *READ *EXECUTE`;
- and, with `*NOQTEMP`, the object is not in `QTEMP`.

`QCRTOBJAUD` gives new objects their `OBJAUD`.

| Type | Value | Written when |
|---|---|---|
| `AF` | `*AUTFAIL` | an authority check fails (`A`), a special authority is missing (`K`), a user-state program breaks domain or state (`B` privileged import, `C` program validation, `D` system-state call or system-domain object), `SBMJOB USER` is refused (`J`) |
| `AD` | `*SECCFG` | `CHGOBJAUD`, `CHGUSRAUD` |
| `AU` | `*SECCFG` | an object's text changes |
| `CA` | `*SECRUN` | a private, public, group or owner authority, or the list securing an object, changes |
| `CO` | `*CREATE` | an object is created |
| `CP` | `*SECCFG` | `CRTUSRPRF`, `CHGUSRPRF`, `DLTUSRPRF`, `CHGPWD` |
| `DO` | `*DELETE` | an object is deleted |
| `DS` | `*SECCFG` | DST resets a service tools password |
| `GR` | `*SECRUN` | the password validation program is called |
| `JD` | `*SECRUN` | a job description's `USER` changes |
| `JS` | `*JOBBAS` | a job starts, ends, is submitted, held, released, changed or ended by `ENDJOB` |
| `OM` | `*OBJMGT` | an object is moved or renamed |
| `OW` | `*SECRUN` | an object's owner changes |
| `PA` | `*SECRUN` | a program that adopts its owner is created |
| `PG` | `*SECRUN` | an object's primary group changes |
| `PS` | `*SECVFY` | a profile handle is obtained, used to swap, or released |
| `PW` | `*AUTFAIL` | a sign-on or API names a user that does not exist (`U`), a wrong password (`P`), a service tools user ID or password that is wrong (`X`) |
| `SE` | `*SECCFG` | a routing entry is added, changed or removed |
| `SF` | `*SPLFDTA` | a spooled file is read, held, released, changed or deleted |
| `ST` | `*SERVICE` | DST or the service console is signed on to |
| `SV` | `*SECCFG` | a system value changes |
| `ZC` | object auditing | an audited object is changed |
| `ZR` | object auditing | an audited object is read |

The restore family (`OR`, `RA`, `RO`, `RP`, `RU`, `RZ`) waits for save
and restore (Phase 11); `SO`, `VA`, `VO`, `VP`, `VU`, `X0`, `X1` and
`X2` belong to servers and networks QS/400 does not have yet. They are
defined (`sec::audit::Kind`) and never written.

Each entry's data is the type's fields below, fixed-width CP037 text,
named as IBM's model files name them (`QASYxxJ5` without the journal
heading). `CPYAUDJRNE` puts the heading and these fields in its output
files; `DSPAUDJRNE` prints them.

Entries about integrated file system objects (Phase 10) carry IBM's
IFS fields at the end of the fixed part: `xxOFID` the object's file ID
(`Qp0lFID_t`, as 32 hexadecimal characters), `xxAPIN` `Y` for an
absolute path and `xxPNLN` the path's length in characters; the path
itself, of any length, follows the fixed part as a `rec::Tail` item
with tag 1 (`xxPNM` in IBM's files), the new path of a move or rename
as item 2 (`OM2PN`). The object fields then hold the object's last name
and its IFS type (`*STMF`, `*DIR` ...) with no library. Entries about
library objects leave these fields blank and have no tail.

### When an entry cannot be written

A receiver past its threshold under `MNGRCV(*USER)` is full. Then:

- `QAUDENDACN(*NOTIFY)`: `CPI2283` goes to `QSYSOPR` (and `QSYSMSG` when
  it exists) once per receiver, and the entries are lost;
- `QAUDENDACN(*PWRDWNSYS)`: `CPI2284` goes to `QSYSOPR`, the service
  marks the next IPL to start with `QAUDCTL(*NONE)` (data area
  `QSYS/QAUDFORCE`), and the system powers down at once. At that IPL
  `QAUDCTL` is set to `*NONE`, `CPI2284` is logged again in `QHST`, and
  auditing stays off until the security officer attaches a new receiver
  and sets `QAUDCTL` again. This is IBM's behavior and a trap: a full
  receiver stops the system; keep `MNGRCV(*SYSTEM)` on `QAUDJRN` or watch
  its threshold.

### `AuditAF` (135 bytes)

Authority failure.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | AFVIOL | violation type, `A`-`Z` |
| 1 | 10 | AFONAM | object name |
| 11 | 10 | AFOLIB | object library |
| 21 | 8 | AFOTYP | object type |
| 29 | 24 | AFOPER | operation (`required.toml`) |
| 53 | 16 | AFSTEP | the search step that decided |
| 69 | 8 | AFHAVE | authority found, hexadecimal bits |
| 77 | 10 | AFPGMN | program, when one was running |
| 87 | 10 | AFPGML | its library |
| 97 | 32 | AFOFID | file ID of an IFS object, hexadecimal |
| 129 | 1 | AFAPIN | `Y` when the path is absolute (IFS) |
| 130 | 5 | AFPNLN | path length (IFS); the path is tail item 1 |

### `AuditAD` (267 bytes)

Auditing change.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | ADETYP | `O` object, `U` user |
| 1 | 10 | ADONAM | object name |
| 11 | 10 | ADOLIB | object library |
| 21 | 8 | ADOTYP | object type |
| 29 | 10 | ADUSRP | the user audited, `U` entries |
| 39 | 10 | ADOVAL | old `OBJAUD` |
| 49 | 10 | ADNVAL | new `OBJAUD` |
| 59 | 10 | ADCMDN | command |
| 69 | 160 | ADALVL | new `AUDLVL`, sixteen values of ten |
| 229 | 32 | ADOFID | file ID of an IFS object, hexadecimal |
| 261 | 1 | ADAPIN | `Y` when the path is absolute (IFS) |
| 262 | 5 | ADPNLN | path length (IFS); the path is tail item 1 |

### `AuditAU` (139 bytes)

Attribute change.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | AUETYP | `T` text |
| 1 | 10 | AUONAM | object name |
| 11 | 10 | AUOLIB | object library |
| 21 | 8 | AUOTYP | object type |
| 29 | 10 | AUATTR | attribute |
| 39 | 50 | AUOVAL | old value |
| 89 | 50 | AUNVAL | new value |

### `AuditCA` (113 bytes)

Authority change.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | CAETYP | `A` granted, `R` revoked, `S` set |
| 1 | 10 | CAONAM | object name |
| 11 | 10 | CAOLIB | object library |
| 21 | 8 | CAOTYP | object type |
| 29 | 10 | CAUSRP | profile, `*PUBLIC` or `*OWNER`, `*PGP` |
| 39 | 10 | CAAUTL | authorization list |
| 49 | 8 | CAOAUT | old bits, hexadecimal |
| 57 | 8 | CANAUT | new bits, hexadecimal |
| 65 | 10 | CACMDN | command |
| 75 | 32 | CAOFID | file ID of an IFS object, hexadecimal |
| 107 | 1 | CAAPIN | `Y` when the path is absolute (IFS) |
| 108 | 5 | CAPNLN | path length (IFS); the path is tail item 1 |

### `AuditCO` (95 bytes)

Object created.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | COETYP | `N` new |
| 1 | 10 | COONAM | object name |
| 11 | 10 | COOLIB | object library |
| 21 | 8 | COOTYP | object type |
| 29 | 10 | COOWNR | owner |
| 39 | 10 | COPGPN | primary group |
| 49 | 8 | COPUBA | `*PUBLIC` bits, hexadecimal |
| 57 | 32 | COOFID | file ID of an IFS object, hexadecimal |
| 89 | 1 | COAPIN | `Y` when the path is absolute (IFS) |
| 90 | 5 | COPNLN | path length (IFS); the path is tail item 1 |

### `AuditCP` (153 bytes)

User profile changed.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | CPCMDN | `CRTUSRPRF`, `CHGUSRPRF`, `DLTUSRPRF`, `CHGPWD` |
| 10 | 10 | CPUSRP | profile |
| 20 | 1 | CPPWDC | `Y` password changed |
| 21 | 1 | CPPWDN | `Y` password `*NONE` |
| 22 | 1 | CPPWDX | `Y` password expired |
| 23 | 10 | CPSTAT | `STATUS` |
| 33 | 10 | CPCLAS | `USRCLS` |
| 43 | 80 | CPSPCA | `SPCAUT`, eight values of ten |
| 123 | 10 | CPGRPP | `GRPPRF` |
| 133 | 10 | CPOWNR | `OWNER` |
| 143 | 10 | CPGAUT | `GRPAUT` |

### `AuditDO` (77 bytes)

Object deleted.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | DOETYP | `A` deleted |
| 1 | 10 | DOONAM | object name |
| 11 | 10 | DOOLIB | object library |
| 21 | 8 | DOOTYP | object type |
| 29 | 10 | DOOWNR | owner |
| 39 | 32 | DOOFID | file ID of an IFS object, hexadecimal |
| 71 | 1 | DOAPIN | `Y` when the path is absolute (IFS) |
| 72 | 5 | DOPNLN | path length (IFS); the path is tail item 1 |

### `AuditDS` (11 bytes)

Service tools password reset.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | DSUSRP | service tools user |
| 10 | 1 | DSRTYP | `R` reset to default |

### `AuditGR` (42 bytes)

Generic record.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | GRETYP | `F` exit program called |
| 1 | 20 | GREXPT | exit point |
| 21 | 10 | GREXPN | exit program |
| 31 | 10 | GREXPL | its library |
| 41 | 1 | GRRTCD | return code |

### `AuditJD` (40 bytes)

Job description USER changed.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | JDONAM | object name |
| 10 | 10 | JDOLIB | object library |
| 20 | 10 | JDOUSR | old `USER` |
| 30 | 10 | JDNUSR | new `USER` |

### `AuditJS` (88 bytes)

Job change.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | JSETYP | `S` start, `E` end, `B` submit, `H` hold, `R` release, `C` change, `N` ended by `ENDJOB` |
| 1 | 1 | JSJTYP | job type |
| 2 | 10 | JSJNAM | job name |
| 12 | 10 | JSJUSR | job user |
| 22 | 6 | JSJNBR | job number |
| 28 | 10 | JSORUS | the user acting |
| 38 | 20 | JSJOBD | job description, `LIB/NAME` |
| 58 | 20 | JSJOBQ | job queue, `LIB/NAME` |
| 78 | 10 | JSCMDN | command |

### `AuditOM` (92 bytes)

Object moved or renamed.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | OMETYP | `M` moved, `R` renamed |
| 1 | 10 | OMONAM | object name |
| 11 | 10 | OMOLIB | object library |
| 21 | 8 | OMOTYP | object type |
| 29 | 10 | OMNNAM | new name |
| 39 | 10 | OMNLIB | new library |
| 49 | 32 | OMOFID | file ID of an IFS object, hexadecimal |
| 81 | 1 | OMAPIN | `Y` when the path is absolute (IFS) |
| 82 | 5 | OMPNLN | path length (IFS); the path is tail item 1 |
| 87 | 5 | OM2PNL | new path length (IFS); the new path is tail item 2 |

### `AuditOW` (86 bytes)

Owner changed.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | OWONAM | object name |
| 10 | 10 | OWOLIB | object library |
| 20 | 8 | OWOTYP | object type |
| 28 | 10 | OWOOWN | old owner |
| 38 | 10 | OWNOWN | new owner |
| 48 | 32 | OWOFID | file ID of an IFS object, hexadecimal |
| 80 | 1 | OWAPIN | `Y` when the path is absolute (IFS) |
| 81 | 5 | OWPNLN | path length (IFS); the path is tail item 1 |

### `AuditPA` (49 bytes)

Adopting program changed.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | PAETYP | `A` adopting program created |
| 1 | 10 | PAONAM | object name |
| 11 | 10 | PAOLIB | object library |
| 21 | 8 | PAOTYP | object type |
| 29 | 10 | PAOWNR | owner |
| 39 | 10 | PAUPRF | `USRPRF` |

### `AuditPG` (94 bytes)

Primary group changed.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | PGONAM | object name |
| 10 | 10 | PGOLIB | object library |
| 20 | 8 | PGOTYP | object type |
| 28 | 10 | PGOPGP | old primary group |
| 38 | 10 | PGNPGP | new primary group |
| 48 | 8 | PGPAUT | its bits, hexadecimal |
| 56 | 32 | PGOFID | file ID of an IFS object, hexadecimal |
| 88 | 1 | PGAPIN | `Y` when the path is absolute (IFS) |
| 89 | 5 | PGPNLN | path length (IFS); the path is tail item 1 |

### `AuditPS` (31 bytes)

Profile swap.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | PSETYP | `H` handle obtained, `S` swapped, `E` handle released |
| 1 | 10 | PSUSRP | from user |
| 11 | 10 | PSNUSR | to user |
| 21 | 10 | PSDEVN | device |

### `AuditPW` (21 bytes)

Password or user not valid.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | PWETYP | `P` password, `U` user, `A` other, `X` service tools user |
| 1 | 10 | PWUSRP | user named |
| 11 | 10 | PWDEVN | device |

### `AuditSE` (85 bytes)

Subsystem routing changed.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | SEETYP | `A` added, `C` changed, `R` removed |
| 1 | 10 | SESBSN | subsystem description |
| 11 | 10 | SESBSL | its library |
| 21 | 4 | SESEQN | sequence number |
| 25 | 10 | SEPGMN | program |
| 35 | 10 | SEPGML | its library |
| 45 | 10 | SECLSN | class |
| 55 | 10 | SECLSL | its library |
| 65 | 20 | SECMPD | compare data |

### `AuditSF` (73 bytes)

Spooled file action.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | SFETYP | `R` read, `D` deleted, `H` held, `L` released, `A` attributes changed |
| 1 | 10 | SFFILE | spooled file |
| 11 | 6 | SFSPNB | its number |
| 17 | 10 | SFJNAM | job name |
| 27 | 10 | SFJUSR | job user |
| 37 | 6 | SFJNBR | job number |
| 43 | 10 | SFOUTQ | output queue |
| 53 | 10 | SFOQLB | its library |
| 63 | 10 | SFOWNR | the file's user |

### `AuditST` (13 bytes)

Service tools used.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | STETYP | `A` used |
| 1 | 2 | STSRVT | `DS` DST sign-on, `SC` service console sign-on |
| 3 | 10 | STUSRP | service tools user |

### `AuditSV` (511 bytes)

System value changed.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 1 | SVETYP | `A` changed |
| 1 | 10 | SVSYSV | system value |
| 11 | 250 | SVNVAL | new value |
| 261 | 250 | SVOVAL | old value |

### `AuditZC` (91 bytes)

Object changed.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | ZCONAM | object name |
| 10 | 10 | ZCOLIB | object library |
| 20 | 8 | ZCOTYP | object type |
| 28 | 1 | ZCACCY | `C` change |
| 29 | 24 | ZCOPER | operation |
| 53 | 32 | ZCOFID | file ID of an IFS object, hexadecimal |
| 85 | 1 | ZCAPIN | `Y` when the path is absolute (IFS) |
| 86 | 5 | ZCPNLN | path length (IFS); the path is tail item 1 |

### `AuditZR` (91 bytes)

Object read.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | ZRONAM | object name |
| 10 | 10 | ZROLIB | object library |
| 20 | 8 | ZROTYP | object type |
| 28 | 1 | ZRACCY | `R` read |
| 29 | 24 | ZROPER | operation |
| 53 | 32 | ZROFID | file ID of an IFS object, hexadecimal |
| 85 | 1 | ZRAPIN | `Y` when the path is absolute (IFS) |
| 86 | 5 | ZRPNLN | path length (IFS); the path is tail item 1 |

## 8. Security system values

Changing a value in the `SEC` group needs `*ALLOBJ` and `*SECADM`; the
auditing values (`QAUDCTL`, `QAUDENDACN`, `QAUDFRCLVL`, `QAUDLVL`,
`QAUDLVL2`, `QCRTOBJAUD`) need `*AUDIT`; any other value `*ALLOBJ`
until `CHGSYSVAL` is a command. DST's "Allow system value security
changes" (`N`) refuses every security change (`CPF18B4`). Each change
writes an `SV` entry and `CPF1806` in `QHST` with the old and new value.

| Value | Behavior |
|---|---|
| `QSECURITY` | §6; takes effect at the next IPL |
| `QALWUSRDMN` | user spaces and indexes created in a library it does not list are system domain |
| `QAUDCTL`, `QAUDLVL`, `QAUDLVL2`, `QAUDENDACN`, `QCRTOBJAUD` | §7; `QAUDCTL` other than `*NONE` needs `QSYS/QAUDJRN` (`CPF2283`); the lists take only their tokens (`CPF1028`) |
| `QAUDFRCLVL` | stored; every audit entry is written before the request answers |
| `QCRTAUT` | `*PUBLIC` of new objects in a library whose `CRTAUT` is `*SYSVAL` |
| `QDSPSGNINF`, `QINACTITV`, `QINACTMSGQ`, `QDSCJOBITV`, `QLMTDEVSSN`, `QLMTSECOFR`, `QMAXSIGN`, `QMAXSGNACN` | §9, sign-on |
| `QPWD*`, `QPWDRULES` | §2; `QPWDRULES` takes only its tokens; `QPWDLVL` takes effect at the next IPL |
| `QUSEADPAUT` | §5 |
| `QALWOBJRST`, `QVFYOBJRST`, `QFRCCVNRST` | stored until restore (Phase 11) |
| `QAUTOCFG`, `QAUTOVRT`, `QRMTSIGN`, `QRETSVRSEC`, `QSHRMEMCTL`, `QSCANFS`, `QSCANFSCTL`, `QSSLCSL`, `QSSLCSLCTL`, `QSSLPCL` | stored; their subjects (devices, servers, file system scans, TLS) come later |

## 9. Signing on

The console's Sign On display sends the user and password to the
storage service (`sign_on`, request 126), which checks them
(`sec::signon`):

| Condition | Message | Audit |
|---|---|---|
| no such user, or the password is wrong | `CPF1107`, the same text for both | `PW` `U` or `P` |
| the wrong password once more would reach `QMAXSIGN` | `CPF1392` | `PW` `P` |
| `QMAXSIGN` reached | `CPF1393`; `QMAXSGNACN` 2 or 3 disables the profile, 1 or 3 varies the device off | `PW` `P` |
| `STATUS(*DISABLED)`, or `USREXPDATE` passed (which disables) | `CPF1394` | |
| `PASSWORD(*NONE)` | `CPF1118` | |
| `QLMTSECOFR` 1, a user with `*ALLOBJ` or `*SERVICE` without `*CHANGE` (owned or private, not public) to the device description | `CPF1110` | |
| `LMTDEVSSN`/`QLMTDEVSSN` and the user already has an interactive job | `CPF1220` | |

`QSECOFR` may always sign on at the console: it is never disabled there
and `QLMTSECOFR` does not apply. The shipped `*DEVD QSCONSOLE` gives
`QSECOFR` `*CHANGE`. A good sign-on clears the count, records the
previous sign-on and makes the user's message queue in `QUSRSYS` when
it is missing.

The interactive job then starts, as on IBM i, with what the sign-on
found: an expired password first shows Change Password (current, new,
new again), which must succeed before anything else runs; it calls
`QPWDVLDPGM` in the job and changes the password through the service
(`chg_pwd`, request 127). `DSPSGNINF`/`QDSPSGNINF` shows Sign-on
Information (the previous sign-on, the attempts not valid, the days
the password has left when it warns). The job takes `INLPGM`,
`INLMNU`, `CURLIB` and, for a workstation entry with `JOBD(*USRPRF)`,
`JOBD` from the profile; the sign-on display's program, menu and
current library override them unless `LMTCPB` says not (`*PARTIAL`
keeps the program and current library, `*YES` all three). `DLVRY(*BREAK)`
puts the user's queue in break delivery. `LMTCPB(*YES)` limits the
command line to `SIGNOFF`, `DSPMSG`, `DSPJOB`, `DSPJOBLOG` and `WRKMSG`
until `*CMD` objects carry `ALLOW(*LIMITED)`.

`INLMNU(MAIN)` shows the interim MAIN menu (golden
`tools/conformance/golden/main-interim.txt`): options 1 to 11 say menus
are not available in this release, 90 signs off, the command line runs
commands (`CALL QCMD` opens Command Entry, whose F3 returns), F9
retrieves the last command, F4 says prompting is not available.
`INLMNU(*SIGNOFF)` signs off when the initial program returns.

`SIGNOFF LOG(*LIST)` keeps the job log; `DROP` and `ENDCNN` have no
connection to drop at the console. `DSCJOB` frees the display and keeps
the job; signing on again as the same user reconnects to it.
`QINACTITV` minutes without input: `QINACTMSGQ(*ENDJOB)` ends the job,
`*DSCJOB` disconnects it, a queue name gets `CPI1126`. A disconnected
job ends after `QDSCJOBITV` minutes.

The serial service console needs a service tools user ID (`signon
USER PASSWORD`, checked as DST checks it, audited `ST` `SC` or `PW`
`X`). Development images (`qs400.svc=1`) sign it on by themselves for
the harness; a `--release` image never has that flag. DST sign-ons
(`ST` `DS`) and password resets (`DS`) are kept by `slic` until the
storage service runs, then audited (`audit_lic`, request 128).
