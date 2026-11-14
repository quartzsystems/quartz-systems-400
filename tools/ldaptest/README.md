# ldaptest

The LDAP test client for QS/400's directory server (`docs/DIRSRV.md`
section 13): scenarios a client runs against a server, each printing
`ok NAME` (with indented lines of what it saw), `fail NAME: reason`, or
`skip NAME: reason` when the target does not advertise what it needs or
the command line gives no identity for it.

## Independent of the server

ldaptest must not share a codec with the server it tests, or a misreading
of an RFC could hide in both. Nothing in its binary depends on `net/ldap`
or `net/dirsrv`; when the server exists, it will appear only in the tests
(dev-dependencies), to be brought up on loopback.

| Part | Where it comes from |
|---|---|
| BER, LDAPv3 messages, the string filter encoder, bind, search, compare, add, modify (with increment), modify DN, delete, abandon, extended, SASL EXTERNAL | `ldap3` 0.12.1 (MIT or Apache-2.0, Ivan Nejgebauer) and its `lber` 0.5.1 |
| Paged results, manageDsaIT, proxied authorization, assertion, pre- and post-read controls; whoami, password modify and StartTLS operations | `ldap3` 0.12.1 |
| TLS (StartTLS and LDAPS, client certificates) | `rustls` 0.23.46 over `rustls-graviola` 0.4.0 (pure Rust; the same provider as `net/tls`), installed by ldaptest since `ldap3`'s provider features bring `ring` or `aws-lc` |
| Server-side sort request and response (RFC 2891), persistent search and entry change notification (draft-ietf-ldapext-psearch-03), tree delete | hand-written, `src/ctrl.rs` over `src/ber.rs` |
| LDIF content records, base64, folding (RFC 2849) | hand-written, `src/ldif.rs` |
| Schema descriptions (RFC 4512 section 4.1) | hand-written, `src/schema.rs` |
| Result code names | hand-written, `src/code.rs` |

All pinned with `=`, pure Rust: no `ring`, no `aws-lc`, no C. The graph has
`tokio`, which `ldap3` runs on (its `sync` wrapper starts a runtime per
connection). `rasn-ldap` was the other candidate; `ldap3` was taken for
having the operations, the filter encoder and most controls already, with
TLS left to the caller.

ldaptest sits outside the workspace with its own `Cargo.lock` (the root
`Cargo.toml`'s `exclude`), as `tools/nfstest` does; `cargo run -p build --
deny` checks its graph (`tools/build/src/product.rs`).

## Command line

```
ldaptest HOST [options]                 run the scenarios
ldaptest oracle [--out FILE] [options]  the anonymous read-only scenarios against the primary
ldaptest bench HOST [--binds N] [--entries M] [options]
ldaptest ldif gen N PARENT              N generated entries as LDIF on stdout
ldaptest --list                         every scenario, its suite, `oracle` when oracle-safe
```

- `--port N` (389), `--ldaps-port N` (636), `--timeout SECS` (10, per
  operation).
- `--suite read,controls,ext,write,acl,projected,ldif` (default all);
  `--only A,B` runs the named scenarios whatever the suites.
- `--admin-dn DN` (default `cn=Administrator`, IBM's `ibm-slapdAdminDN`);
  its password from `LDAPTEST_ADMIN_PW`. Without it every scenario that
  writes is a `skip`.
- `--suffix DN`: the suffix the writing scenarios work under. Each makes
  `ou=ldaptest-NAME-PID,SUFFIX` and removes it and everything below it at
  the end.
- `--profile NAME` (password from `LDAPTEST_PROFILE_PW`) and
  `--disabled-profile NAME` (`LDAPTEST_DISABLED_PW`) for the projected
  backend; `--system NAME` the `os400-sys=` value (default the root DSE's
  `ibm-osregistrycontext`).
- `--ca FILE` trusts that PEM certificate; without it any server
  certificate is taken (the handshake's signatures are still checked).
  `--client-cert FILE --client-key FILE` (PEM) for SASL EXTERNAL.
- `--entries N` (200) entries made by `paged-many`, `sort-many`,
  `ldif-round-trip` and `bench`; `--page N` (50) the page size;
  `--binds N` (1000) for `bench`.
- The exit code is the number of failed scenarios (at most 100); 101 is a
  usage error. `bench` prints binds a minute (a connection each, and all
  on one connection), adds a second and a paged search's time and rate
  over the entries it made (with `--suffix` and the administrator;
  otherwise an anonymous paged search of the first base).

## Scenarios

`ldaptest --list` prints them. `oracle` marks the ones that are
anonymous, write nothing and bind with no password; `ldaptest oracle`
runs only those.

- read (all oracle-safe but the last): `rootdse` (every attribute and its
  count, versions, naming contexts, controls and extensions, distinct and
  repeated), `rootdse-default-attrs` (no list, `+` only, one attribute),
  `rootdse-one-level`, `schema-read` (counts of each kind, every value
  parsed, specific OIDs: `cn`, `objectClass`, `person`, `inetOrgPerson`,
  `referral`, the ACL attributes, `os400-root`, `os400-usrprf`),
  `naming-contexts`, `search-scopes` (base, one, sub of `cn=localhost` and
  `cn=ibmpolicies`), `filters` (37 forms: presence, equality in both
  cases, substrings of every shape, `>=`, `<=`, `~=`, `&`, `|`, `!`,
  extensible match with rule, OID and `:dn:`, undefined attributes,
  escapes, empty value, `(&)` and `(|)`, operational attributes),
  `size-limit`, `time-limit`, `types-only`, `attr-selection` (`*`, `+`,
  `1.1`, `1.1` with a name, unknown and operational names, none),
  `compare`, `no-such-object` (matched DN), `invalid-dn`, `anon-bind`,
  `special-subtrees` (`cn=monitor`, `cn=configuration`, `cn=changelog`),
  `unauth-bind` (a name with no password).
- controls: `paged-results`, `paged-outstanding` (several left open on
  one connection), `paged-abandon`, `paged-bad-cookie`, `sort`
  (ascending, descending, two keys, an ordering rule, an unknown
  attribute critical and not, four keys), `sort-paged`, `manage-dsa-it`,
  `psearch-accept` (initial entries, abandon), `proxied-authz-anon`,
  `assertion-read`, `unknown-control` (all oracle-safe); writing:
  `paged-many`, `sort-many`, `referrals` (a referral object: continuation
  references, 10 on and below it, the entry with manageDsaIT),
  `persistent-search` (add, modify, modify DN and delete seen with their
  entry change notifications), `proxied-authz` (the administrator acting
  as a user is refused what the user may not do; a user acting as the
  administrator 123), `tree-delete` (66 without, gone with; anonymous
  50), `assertion-write` (122), `pre-post-read`.
- ext: `unknown-extop`, `whoami-anon`, `starttls` (and a second StartTLS,
  and one with a value), `ldaps` (oracle-safe); `password-modify-anon`,
  `sasl-external` (StartTLS and LDAPS, whoami gives the subject),
  `sasl-external-no-cert`, `whoami-bound`, `password-modify` (own change,
  wrong old password, administrator reset with a generated password,
  anonymous refused).
- write: `add-entry` (UTF-8 values, found by each), `add-errors` (68, 32,
  65, 17, 64, 21, 34), `modify-ops` (add, delete value, replace, delete
  attribute, all-or-nothing, increment), `modify-errors` (16, 20, 67, 65,
  17, 32, 19, 69), `moddn-rename` (with and without deleteOldRDN, onto an
  existing name), `moddn-subtree-move` (7 entries, under itself refused),
  `delete-entry` (66, 32), `anon-write-denied` (50 for each),
  `operational-attrs` (`creatorsName` is the administrator).
- acl (docs/DIRSRV.md §12): `acl-default` (anonymous sees normal
  attributes, not `userPassword`; the administrator does), `acl-grant`
  (`object:ad:normal:rwsc` for one user), `acl-deny` (`at.description:deny`
  for one user, not for anonymous), `acl-propagate` (`aclPropagate:
  FALSE`), `acl-owner` (`entryOwner` gives every right below),
  `acl-sources` (`aclSource`, `ownerSource`, `ibm-effectiveAcl`),
  `bind-errors` (49 for a wrong password, a missing entry and an entry
  without one).
- projected (§9): `projected-anon` (oracle-safe: the root only),
  `projected-bind`, `projected-disabled` (49), `projected-read-only` (53),
  `projected-admin-view` (every profile, `os400-usrprf`).
- ldif: `ldif-round-trip` (N generated entries written as LDIF, read
  back and added; the subtree exported, written, read and compared, base64
  values included).

## Oracle mode

`ldaptest oracle` reads the primary's address from `QS_ORACLE_HOST` (the
variable in `tools/conformance/oracle/.env`; ldaptest reads the
environment only and never prints it) and runs the oracle-safe scenarios
on port 389. Every connection of the run is read-only in the client
itself: a bind other than the anonymous one, and every add, modify,
modify DN, delete or SASL bind, is refused before anything is sent
(`client.rs`, and its test). The report is written to
`tools/conformance/oracle/dirsrv/ldaptest-primary.txt` with the system's
names masked as `{system}`, `{host}`, `{domain}` and `{server-id}`; the
run refuses to write a record that still holds one. Lines with `diag:`
are the server's own wording, which a diff of two servers masks.

```
export QS_ORACLE_HOST=...   # from tools/conformance/oracle/.env
cargo run --manifest-path tools/ldaptest/Cargo.toml -- oracle
```

QS/400's answers are to be diffed against that file (the documented
differences of docs/DIRSRV.md §15 aside).

## Tests

`cargo test --manifest-path tools/ldaptest/Cargo.toml`: BER, the sort,
persistent search and entry change control values, LDIF (RFC 4648 and
RFC 2849 vectors, folding, base64 round trips), schema descriptions,
masking, the command line, and the read-only guard against a listener
that never answers. The loopback tests against QS/400's server are added
with the server, which is not built yet.

## What the primary showed (2026-11-14)

Beyond `README.md` beside the record, ldaptest's run settled:

- The root DSE's attributes are user attributes to IBM: a search with no
  list returns them all, `+` alone returns none.
- One-level of `""` is 53; subtree of `""` returns the five entries of
  `cn=localhost` and `cn=ibmpolicies`; one-level of `cn=schema` is 2.
- Filters: extensible match (with a rule, an OID or `:dn:`) matches
  nothing; `(&)` and `(|)` match nothing; `(aclEntry=*)` and
  `(entryOwner=*)` are 53 for anonymous users; `~=` is approximate
  (`(cn~=ibmpolicy)` finds `cn=ibmpolicies`).
- Limits: a size limit below the count is 4 with that many entries; a
  time limit of 1 s on a small search is 0.
- Paged results: a page size of 0 with the cookie ends the search (an
  empty cookie back); a cookie the server did not make is 53; a second
  paged search started while one is open on the connection is 80, and
  one more after that 53 (`paged-outstanding`).
- Sort: an unknown attribute is 12 when critical and 0 with sortResult 16
  when not; four keys (the limit is 3) are 0 with sortResult 11.
- Assertion, critical: 12 (not supported, as the root DSE says);
  non-critical: ignored. An unknown critical control is 12; the paged
  results control on a compare, critical, is 12.
- Proxied authorization from an anonymous user is 50 whatever the
  identity.
- Persistent search: the initial entries come; right after an abandon the
  next request on that connection is 53, a second later 0.
- An unknown extended operation is 2 "unsupported extended operation"
  with no response name; whoami is the same; StartTLS is 2 (no
  certificate); 636 refuses the connection.
- The projected backend: `cn=accounts` and any profile's DN are 0 with no
  entries for anonymous users (not 32).
- 32 has the matched DN in IBM's upper case (`CN=LOCALHOST`).
