# PTFs

A program temporary fix replaces objects of a licensed program, and
sometimes the Licensed Internal Code, without reinstalling it. QS/400
PTFs are named `QS` and five digits (`QS00001`), cumulative packages
`C` + `yyddd` + `110`, PTF groups `QS99nnn`, as the `release` crate
defines. This document is the PTF format (Quartz's own), the states
(IBM's), how apply and remove work, and the slots a LIC PTF uses.
`ptf/ptf/tests/spec.rs` checks the layouts against the tables here.

## 1. Format

A PTF is a save stream (`docs/SAVE_FORMAT.md`, kind 8) and nothing else:

1. object 0, type `0xFFF0` (`*PTFHDR`, never stored): typed part 16 is
   the `PtfHeader` with its lists, typed part 17 the cover letter
   (UTF-8 text, Quartz-written);
2. the replacement objects, in save order, each with its library;
3. with a LIC replacement: an object of type `0xFFF1` (`*LICIMG`, never
   stored) whose space part is the new UKI;
4. data migration programs: ordinary `*PGM` objects that the header
   names.

Every object is signed (`SAVE_FORMAT.md` §5) by the key the system
trusts for system objects; the header object too, so its digest covers
the header and the cover letter. A PTF whose objects do not verify is
not loaded (`LODPTF`, `CPF3586`).

### `PtfHeader` (128 bytes)

Then `prereq_count` × `PtfRef`, `coreq_count` × `PtfRef`,
`supersedes_count` × `PtfRef`, `exit_count` × `ExitProgram`.

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | magic | `QS400PTF`, ASCII |
| 8 | 2 | version | 1 |
| 10 | 1 | kind | 0 immediate, 1 delayed (applied at the next IPL) |
| 11 | 1 | flags | bit 0 replaces the LIC, bit 1 has migration programs, bit 2 cover letter only |
| 12 | 8 | ptf | `QSnnnnn`, ASCII |
| 20 | 7 | product | product ID, `QS00SS1`, ASCII |
| 27 | 4 | option | `0000` for `*BASE`, ASCII |
| 31 | 6 | release | `VxRyMz`, ASCII |
| 37 | 4 | load | load ID, `5050` for code, ASCII |
| 41 | 6 | target_release | the oldest release it applies to, ASCII |
| 47 | 1 | reserved | 0 |
| 48 | 8 | created | Unix seconds |
| 56 | 2 | prereq_count | |
| 58 | 2 | coreq_count | |
| 60 | 2 | supersedes_count | |
| 62 | 2 | exit_count | |
| 64 | 4 | object_count | replacement objects in the stream |
| 68 | 4 | store_format | the store format version a migration program brings the system to, 0 for none |
| 72 | 56 | reserved2 | 0 |

### `PtfRef` (16 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 8 | ptf | `QSnnnnn`, ASCII |
| 8 | 7 | product | ASCII |
| 15 | 1 | reserved | 0 |

### `ExitProgram` (32 bytes)

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 10 | library | CP037 |
| 10 | 10 | program | CP037 |
| 20 | 1 | when | 0 before apply, 1 after apply, 2 before remove, 3 after remove, 4 data migration |
| 21 | 11 | reserved | 0 |

## 2. States

The status `DSPPTF` shows, IBM's names:

| Status | Meaning here |
|---|---|
| Not loaded | ordered or known, not on the system |
| Loaded | `LODPTF` checked it and keeps it; nothing applied |
| Temporarily applied | its objects are in place; the replaced ones wait in `QRPLOBJ` |
| Permanently applied | applied, and the replaced objects are gone |
| Temporarily removed | removed after a temporary apply; the PTF is still loaded |
| Permanently removed | removed and its save file deleted |
| Superseded | a later PTF that supersedes it is loaded |
| Damaged | its save file or a replaced object did not verify |
| Apply at next IPL | a delayed PTF waiting for the IPL that applies it |
| Remove at next IPL | waiting for the IPL that removes it |
| On order | `SNDPTFORD` asked for it |
| Save file only | its save file is here, not loaded |
| Cover letter only | only the cover letter is here |

## 3. Apply and remove

- `LODPTF` checks the signatures, the product, option and release, and
  the prerequisites that must already be loaded, then keeps the stream
  as save file `QGPL/QSnnnnn` and records the PTF as Loaded. Loading a
  PTF that supersedes others marks those Superseded.
- `APYPTF APY(*TEMP)` of an immediate PTF: each replacement object
  that exists is moved to `QRPLOBJ` under a generated name (its
  original library, name and type recorded), the new object restored
  in its place with its signature; migration programs run; the PTF is
  Temporarily applied. One object failing puts every moved object back.
- A delayed PTF (`DELAYED(*YES)`, or a LIC PTF) becomes Apply at next
  IPL; the IPL applies it before the OS phase (`C6004xxx`, §5).
- `APYPTF APY(*PERM)`: the `QRPLOBJ` copies are deleted.
- `RMVPTF RMV(*TEMP)`: the replacement objects are deleted and the
  `QRPLOBJ` copies moved back: Temporarily removed. `RMV(*PERM)` also
  deletes the save file: Permanently removed. A PTF that another
  applied PTF names as a prerequisite cannot be removed alone.
- Prerequisites must be applied first or in the same request;
  co-requisites are applied together; a superseded PTF is not applied.

## 4. Groups and cumulative packages

A PTF group is a TOML manifest (`QS99nnn`, level, text, PTFs, related
groups) signed by the build key. Its status comes from its PTFs:
Installed when each is applied or superseded, Apply at next IPL when
any waits for the IPL, Not installed when any is missing, Error when
any is damaged, Unknown when the manifest does not verify. A cumulative
package is a group named `Cyyddd110` that lists the current PTF of
every product on the media.

## 5. Slots and IPL codes

A LIC PTF writes its UKI into the inactive slot of the ESP (the A/B
slots, `docs/PHASE1.md`) and records Apply at next IPL; the IPL that
boots the other slot makes the PTF Temporarily applied. Removing it is
an IPL from the slot that still holds the old image; a permanent apply
copies the new image into both slots. A power loss before the apply
IPL finishes leaves the old slot bootable, so the next IPL retries.

| SRC | Meaning |
|---|---|
| `C6004501` | applying delayed PTFs |
| `C6004502` | removing PTFs marked for the next IPL |
| `C6004503` | a LIC PTF took effect from the other slot |
| `C6004504` | a delayed PTF failed to apply; it is Damaged |

## 6. Trusted signers

`LODPTF`, `INSPTF`, the save engine's signature checks, group manifests
and the update service's catalog all verify against one file,
`/qs400/keys/system.pub` in the initramfs: the ed25519 public keys of
the signers the system trusts for its own objects, 32 raw bytes each,
back to back, read on every check. `build initramfs` writes the build
key's first (`QS400_SIGNING_KEY`, or the dev key under `target/keys/`),
then each key of the file `QS400_TRUSTED_KEYS` names.

A third party that ships PTFs (built with `QSCRTPTF` and its own key)
is trusted this way:

1. the vendor sends its public key as 64 hex digits (the ed25519
   public key of its seed);
2. the security officer checks its key ID (the first eight bytes of its
   SHA-256, which every save stream it signs carries) with the vendor
   out of band;
3. the key goes on a line of the `QS400_TRUSTED_KEYS` file (`#` starts
   a comment), and the image is built again: `build all`, or a LIC PTF
   whose UKI carries the new list.

Removing a key is the same with its line deleted. The list is part of
the signed image on purpose: nothing a running system can write widens
whom it trusts.

## 7. `QSCRTPTF` manifests

`QSCRTPTF MANIFEST('/home/me/ptf.toml') KEYFILE('/home/me/key.hex')`
builds a PTF from objects on this system. The key file is the build's
form: the 32-byte ed25519 seed as 64 hex digits (or the 32 raw bytes).
The save file is `QGPL/QSnnnnn` unless `TOSAVF` names another. The
manifest (`ptf::manifest`):

```toml
ptf = "QS00042"              # required
product = "5770SS1"          # required
option = "0000"              # default *BASE
release = "V1R0M0"           # default: this system's
target_release = "V1R0M0"    # default: release
load = "5050"                # default
kind = "immediate"           # or "delayed"
prereqs = ["QS00041"]        # same product
coreqs = []
supersedes = ["QS00030"]
cover = "What the PTF corrects."     # or cover_file = "/path/cover.txt"
lic = "/path/uki-a.efi"      # a LIC PTF: slot A's build of the UKI
lic_b = "/path/uki-b.efi"    # and slot B's (else slot A's serves both)
store_format = 0             # the format a migration program brings

[[object]]                   # each replacement object
library = "QSYS"
name = "QWCSTRUP"
type = "*PGM"

[[exit]]                     # exit and data migration programs
library = "QSYS"
program = "QSMIG42"
when = "migration"           # before-apply, after-apply, before-remove,
                             # after-remove, migration
```

Unknown keys are refused, so a misspelt field does not build a PTF that
silently lacks it. The objects are signed where they are (their headers
carry the signature they are saved with) and saved in library order.
