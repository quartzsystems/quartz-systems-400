# Phase 6: security

Phase 6 gives the system users. A job now runs as a user profile with
groups and special authorities; every object carries an owner, a
primary group, private authorities, `*PUBLIC` and an authorization
list, and the store itself refuses what the user may not do; programs
adopt their owner's authority; `QSECURITY` 40 and 50 keep user-state
programs out of system state; what matters is written to `QAUDJRN`;
the console's sign-on checks a password; and the security commands and
APIs exist.

## What was built

| Crate or place | Holds |
|---|---|
| `sec` (`security/sec`) | the `*USRPRF` payload, passwords (Argon2id, the `QPWD*` rules, history), groups and limits; authority bits, grants, authorization lists, the profile indexes, the search and its cache; the guard the store calls; the audit journal's entries, layouts and writer; the security system values' rules; sign-on; the APIs' questions; the commands as library functions |
| `security/sec/required.toml` | what each operation of the store, the database and work management needs, keyed as the code names it (`obj.delete`, `file.open-input` ...) |
| `security/sec/exempt.toml` | the entry points that check nothing themselves, with the reason |
| `storage/store/src/obj/guard.rs` | the `Guard` trait the store calls before an operation and around creation, deletion and growth; without one the store is open, as it is at IPL |
| `storage/store` header | `owner_authority`, `group_authority`, `auth_gen`, `auth_flags`, the private overflow tree, `ADOPTS` and the domain and state bits |
| `lic/slic/src/stgsvc/security.rs` | the service's side: which requests run as the job, the per-job session, adoption, sign-on and password changes, the audit flush and `QAUDENDACN`, the APIs' handles |
| `lic/slic/src/svc/security.rs`, `authority.rs` | the console's audit, report, profile and authority commands, and `whoami` |
| `runtime/mi` | `MCH6801`: load and call refusals by state, system-domain spaces, `QUSPTRUS`; adoption sent as the call stack changes |
| `runtime/ile` | `USRPRF`, `USEADPAUT`, `STATE` on a program; the privileged procedures |
| `runtime/sysapi/src/security.rs` | `QSYRUSRI`, `QSYCUSRS`, `QSYCUSRA`, `QSYRUSRA`, `QSYLOBJA`, `QSYLOBJP`, `QSYRAUTU`, `QSYLAUTU`, `QSYLATLO`, `QSYGETPH`, `QWTSETP`, `QSYRLSPH`, `QSYCHGPW`, `QSYRUPWD`, `QSYSUPWD` |
| `work/wm`, `work/qsjob` | sign-on through the service, the Change Password and Sign-on Information panels, the interim `MAIN` menu, `SIGNOFF`, `DSCJOB`, `QINACTITV` |
| `conformance` | QEMU steps 37 to 45, the golden sign-on screens, the oracle kit |

`SECURITY.md` describes the model, the layouts, the search, auditing
and sign-on; `SEC_OPEN_QUESTIONS.md` the readings the machine must
settle.

The decisions in force:

- Enforcement lives in the store. Every operation of the store, the
  database and the journal names what it does, the guard looks the
  need up in `required.toml` and asks the search. A job's principal is
  a thread-local the storage service sets for each request; with none
  the work is the system's. A missing check is a test failure
  (`exempt.toml`).
- The search is IBM's: `*ALLOBJ`, then the user's own entry (owner,
  private, list), then the groups' entries together, then `*PUBLIC`,
  then adoption. A private `*EXCLUDE` stops the search. Decisions are
  cached per job and per object generation; a profile or list change
  bumps a global epoch.
- Passwords are Argon2id. Level 0 and 1 keep an upper-case ten-byte
  form, level 2 and 3 the long one; a profile keeps both while the
  level allows the short one.
- Auditing is an event queue flushed after the request that produced
  it, in its own transaction, so a refused request is still recorded.
  A full receiver under `MNGRCV(*USER)` is where `QAUDENDACN` acts.
- Only development images bind `STATE(*SYSTEM)`; the harness uses it
  to test the state checks.

## Numbers

`cargo test --release -p sec --test bench -- --ignored --nocapture`
(in memory, 1,000 objects with twenty private authorities each, fifty
users, 2026-10-22):

| Benchmark | Result |
|---|---|
| reading an object's header | 23.1 µs |
| the authority search, cold | 5.2 µs |
| a cached decision in a job's session | 0.2 µs |

Step 45 of the QEMU suite (KVM on the development machine,
2026-10-22):

| Benchmark | Result |
|---|---|
| sign-on from Enter to the first menu | 169 ms |
| `CHKOBJ` at Command Entry, cold then cached | 429 ms, 449 ms |

The sign-on time is the password check (Argon2id at its parameters),
the job's start and the menu, seen through the console's screen
polled every 20 ms. The `CHKOBJ` round trips are the serial line, the
job's line dispatch and the screen polled every 250 ms; the check
itself is the microseconds above, which is why the cached run is no
faster.

## Tests

- `cargo test -p sec`: the layouts against `SECURITY.md`'s tables
  (`tests/spec.rs`), profiles and passwords, grants and the indexes,
  the search matrix (every source of authority against every need),
  the guard on a store, every checked operation in `required.toml`,
  every unchecked entry point in `exempt.toml`, the audit entries and
  layouts, sign-on, the APIs' questions, the commands.
- `cargo test -p sysapi --test security`: the APIs' parameters and
  receivers over canned answers.
- `cargo run -p build -- test` (WSL): the QEMU suite, 45 steps, at
  `QSECURITY` 40. Phase 6's are 37 to 45: profiles, a list and the
  displays; a user signed on at the console refused what she may not
  do and audited; adopted authority; `MCH6801` for a privileged import
  and a call into system state; object auditing, `CPYAUDJRNE` and a
  printed report; a full receiver with `*NOTIFY`, then `*PWRDWNSYS`
  and the IPL that follows; `DLTUSRPRF`; the suite's subset at 30 and
  50; the numbers above. Phases 1 to 5's steps still pass; they now
  sign `QSECOFR` on with a password.
- The oracle kit (`tools/conformance/oracle/sec/`) was captured on an
  IBM i 7.1 on 2026-10-22 and `cargo test -p conformance --test
  sec_oracle` agrees with it; `sec`'s `shipped_csv` test checks the
  shipped profiles against the machine's `DSPUSRPRF` outfile. Before
  that it waited for a
  machine.

## Unsupported, by design or for now

- `QSYCHGPR` (not identified); `QSYRUPWD` data is not IBM's format and
  moves between QS/400 systems only.
- Level 10 is not offered; level 20 keeps everyone `*ALLOBJ` and the
  first IPL at 30 or above takes it back from every class but
  `*SECOFR`.
- `EDTOBJAUT` and `EDTAUTL` on the serial console show the list; the
  interactive editors wait for display files (`MIGRATE(phase8)`).
- Object auditing is by object and by user; `*USRPRF` on an object
  follows the user's `OBJAUD`, as IBM's does, but the user's `AUDLVL`
  action auditing covers the actions this phase records.
- `*IOSYSCFG`, `*SAVSYS` and `*SERVICE` are kept, listed and checked
  where a command of this phase needs them; what they guard mostly
  waits for save, restore and devices (`MIGRATE(phase9)`,
  `MIGRATE(phase10)`).
- Validation lists, function usage (`WRKFCNUSG`), Kerberos, SSL and
  the digital certificate manager are out of scope.
- `QSECURITY` changes take effect at the next IPL. The first IPL after
  a change needs no scan of programs for state and domain: every
  program on the system was bound here, and the binder sets both from
  the creator's authority at the time, so there is nothing a scan could
  find that the call-time checks do not.

`SEC_OPEN_QUESTIONS.md` lists the readings the machine must settle.
