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

A composition failure is `CPF4AA7` with IBM's reason code:

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
`RVKOLDPGP(*YES)`. An object's owner cannot be its primary group.

### Authorization lists

A `*AUTL` is an object in `QSYS` whose private authorities are the
list's entries and whose `*PUBLIC` is the list's public authority; its
entries therefore live where any object's private authorities do.
`*AUTLMGT` on the list lets a user add and remove entries. Tree root
slot 0 indexes the objects the list secures (`DSPAUTLOBJ`). A list
cannot secure another list.

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
