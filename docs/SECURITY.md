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
