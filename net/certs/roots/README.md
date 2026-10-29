# Shipped root certificates

`cacert.pem` is the Mozilla root CA set as curl extracts it from NSS's
`certdata.txt` with `mk-ca-bundle.pl`.

- Origin: https://curl.se/ca/cacert.pem (https://curl.se/docs/caextract.html)
- Taken: 2026-10-24; "Certificate data from Mozilla as of: Tue Oct 20 13:32:40 2026 GMT"
- SHA-256 of the file: 796f1389bfea13c0ed78f663b765baaff54a508e5483786bd6ece73c5e3e3cbe
- License: the bundle is distributed under the Mozilla Public License 2.0
  (MPL-2.0), as noted on the extract page; `deny.toml` allows MPL-2.0.

The file is data, embedded by `net/certs/src/roots.rs`; only the
`CERTIFICATE` blocks and the title line above each are read. The titles
become the labels of the trusted CA entries seeded into the `*SYSTEM`
store on its creation.

## Updating

1. Download the new `cacert.pem` and `cacert.pem.sha256` from curl.se and
   check the hash.
2. Replace this file and the three lines above.
3. `cargo test -p certs roots` (the bundle must still parse to more than
   100 self-signed CA roots with unique titles).
4. An installed system picks the new set up through `certs::roots::seed`
   on the next first-IPL seeding or when a PTF runs it; existing entries
   are kept (matched by SHA-256 fingerprint), new roots are added, and
   roots Mozilla dropped are not removed automatically (the administrator
   removes them, since a site may rely on one).
