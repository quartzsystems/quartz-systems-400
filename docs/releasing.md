# Releasing

`VERSION`, at the root, holds QS/400's version as MAJOR.MINOR.PATCH, and
the release follows it: `1.1.0` is V1R1M0. `lic/release` reads it at build
time, so every panel, `DSPSFWRSC`, the media's volume ID, the release
directory (`target/release/V1R1M0/`), the ISO's name
(`QS400-V1R1M0-<hash>.iso`) and the GitHub release all come from it. A
test build still claims another release with `QS400_RELEASE=VxRyMz`
(the upgrade path, `INSTALL.md` §4). The crates keep version 0.1.0:
they are not published.

`ci/version.sh check`, run in CI, fails when `lic/release`'s `KNOWN` does
not list the release `VERSION` names (the upgrade rule looks there).

## Changelog

`CHANGELOG.md` has one section per release, newest first. Add each change
under `## Unreleased` as you make it. The release's section becomes its
GitHub release's notes; the full notes are `docs/RELEASE_NOTES_VxRyMz.md`,
which `build release` also puts at the root of the media and into the
Knowledge Center.

## The keys

A release is signed with two keys. Neither is in the repository, in
GitHub's secrets or in a CI log.

| Key | Variable | Signs | If it leaks / is lost |
|---|---|---|---|
| Release signing key, ed25519: a file of 64 hexadecimal characters (the 32-byte seed) | `QS400_SIGNING_KEY` | the media manifest, `SHA256SUMS`, every licensed program and every PTF; installed systems trust its public key | anyone can make products and PTFs installed systems accept / no fix can reach installed systems without a new trust anchor |
| Secure Boot db key, RSA-2048, PKCS#8, and its X.509 certificate | `QS400_SB_KEY`, `QS400_SB_CERT` | the UKIs (Authenticode); `qs400-db.cer` ships on the media for firmware enrolment | anyone can boot code on a machine that enrolled it / every machine that enrolled it must enrol a new certificate |

Make them once, on a trusted machine, into a directory outside the
checkout (an encrypted USB stick, say):

```sh
openssl rand -hex 32 > /media/keys/qs400-release.ed25519
cargo run -p pesign -- keygen /media/keys "CN=Quartz Systems QS/400 Secure Boot db 2026"
```

Keep two copies offline (the stick and a second stick or a password
manager's file vault in another place). Without the variables, `build`
signs with the development keys (an ed25519 key generated under
`target/keys/`, and the checked-in Secure Boot pair
`tools/pesign/keys/qs400-db-DEV-ONLY.*`), and says so; CI and the
nightly build always do.

## Cutting a release

On Windows run `ci\version.ps1` in place of `ci/version.sh`, with the
same arguments (it runs the script with Git for Windows' bash).

1. Prepare it: writes the version to `VERSION` and turns `## Unreleased`
   into `## X.Y.Z (date)` with a new empty Unreleased section above it.
   A new release also goes into `KNOWN` in `lic/release/src/lib.rs`.

   ```sh
   ci/version.sh prepare 1.1.0
   ```

2. Read over the section (`ci/version.sh notes`), then commit and tag,
   one command at a time (Windows PowerShell 5.1 has no `&&`):

   ```sh
   git commit -am "Release 1.1.0 (V1R1M0)"
   git tag v1.1.0
   git push origin main v1.1.0
   ```

Pushing the tag runs `.github/workflows/release.yml`:

1. **version**: the tag is `v` followed by `VERSION`, `KNOWN` lists the
   release, and `CHANGELOG.md` has a non-empty section for it.
2. **ci**: every job of `ci.yml`, the QEMU suite on the `qs400-qemu`
   runner included.
3. **build**: on the `qs400-qemu` runner, `build all --release` and
   `build release`, signed with the keys the runner's own environment
   names. The runner must have `QS400_SIGNING_KEY`, `QS400_SB_KEY` and
   `QS400_SB_CERT` set to files (in the runner's `.env`, beside
   `run.sh`); without them the job stops rather than publish a release
   signed with the development keys.
4. **publish**: a GitHub release named `QS/400 VxRyMz (x.y.z)` with the
   ISO, the UKIs, `SHA256SUMS` and `SHA256SUMS.sig`, `MANIFEST.json` and
   its signature, `NOTICES.txt`, `SOURCE-OFFER.txt` and `qs400-db.cer`,
   and the changelog section as its notes.

A failed run publishes nothing. Fix the problem on main, then move the
tag, or, if the release was published, prepare the next patch version:

```sh
git tag -f v1.1.0
git push -f origin v1.1.0
```

If the keys stay off the runner (the stick is plugged in only to sign),
build and sign by hand instead and publish with `gh release create`:

```sh
QS400_SIGNING_KEY=/media/keys/qs400-release.ed25519 \
QS400_SB_KEY=/media/keys/qs400-db.key QS400_SB_CERT=/media/keys/qs400-db.cer \
  cargo run -p build -- release
```
