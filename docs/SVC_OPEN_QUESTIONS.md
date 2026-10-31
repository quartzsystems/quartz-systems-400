# Phase 11 open questions

Questions Phase 11 answered provisionally or settled by design, with
what QS/400 does now and how to settle each.

## Licensed programs

1. **License key form (resolved).** IBM's license key is 18
   characters, entered as `LICKEY` with three 6-character elements. It
   is a vendor-secret code, not a signature: nothing on the system can
   tell a genuine key from a forged one without IBM's algorithm.
   QS/400 signs keys with ed25519 so that the system checks them
   against a public key and nobody holding the system can mint them;
   a signature is 64 bytes and cannot fit 18 characters. Two forms were
   weighed: a short key carrying a truncated MAC with a signed key file
   beside it, and the whole signature typed by hand. A MAC needs a
   secret on the system, which defeats the point, and a truncated
   signature verifies nothing. **Chosen**: the key is the full
   signature plus a 28-bit typing check, 108 characters in a 32-letter
   alphabet without `I`, `L`, `O` and `U`, printed as six lines of IBM's
   three-group layout and entered as `LICKEY` with 18 elements; the
   fields the key binds (product, term, feature, serial number,
   processor group, usage limit, expiration date, vendor data) stay
   ADDLICKEY's own parameters. Keys are normally delivered as a key
   file for `LICKEYINP(*LICKEYFILE)`; the 108 characters are for
   keying by hand when the file cannot be moved (`docs/LICPGM.md` §7).
2. **QSZRTVPR offsets.** The "offset to additional information" value,
   PRDR0200's next-record offset base, PRDR0500's option record
   length, PRDR0700's release list and PRDR0800's entry layout are
   this implementation's reading of IBM's field lists
   (`docs/LICPGM.md` §4). Settle by calling QSZRTVPR on pub400 for
   `5770SS1 *BASE` and `5770SS1` option 33 in each format and dumping
   the receivers.
3. **Load state symbols for in-progress states.** States 35, 38, 50
   and 53 are given `*PACKAGED`/`*LOADED` and `*ERROR` here. Confirm
   against IBM's QSZRTVPR documentation of the symbolic state.
4. **Threshold notice.** The message sent when license use reaches
   the threshold is assumed to be `CPI9E19`; confirm in `QCPFMSG`.
5. **QSZCRTPL directory and agreement lists.** The optional groups'
   entry formats (`SDIR0100` and the agreement document list) are not
   read; a nonzero count is refused. Read IBM's QSZCRTPL page and add
   them.
6. **Target releases.** QSZCHKTG knows only Quartz releases, so IBM
   release names (`TGTRLS(V7R4M0)`) are refused. Whether QS/400 should
   map IBM target releases onto its own is a compiler question for the
   CRT* commands.
