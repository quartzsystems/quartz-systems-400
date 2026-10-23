# Phase 8 open questions

Semantic questions about the display interface, each with how it was
settled (a probe script under `tools/conformance/probes/` run on an
oracle with `tn5250-driver`) or why it is still open. pub400.com
(V7R5M0) is the only oracle with a reachable telnet; the primary
oracle (V7R1M0) answers no screen questions.

1. **The seed in `NEW-ENVIRON SEND`.** Resolved 2026-10-23, trace of
   the first connection (`--trace --hex`): the server sends `USERVAR
   "IBMRSEED"` with the eight seed bytes appended to the name, no
   `VALUE` byte. Implemented on both sides; the client also accepts a
   `VALUE` form.
2. **Order codes.** Resolved 2026-10-23, same trace: Start of Header is
   `0x01` with a seven-byte body whose fourth byte is the error row
   (24), Transparent Data `0x10`, Move Cursor `0x14`.
3. **Record length.** Resolved: the two-byte length counts the whole
   record including itself (791 for a record of 791 bytes).
4. **The start-up response record.** Open. pub400 sends none when no
   `DEVNAME` is asked for. QS/400 sends `I902` + the device name after
   the header when a device was named, per RFC 2877 as read; the
   bytes a real system sends to ACS could not be captured (no telnet
   on the primary oracle). Needs Cody: an ACS trace, or a session to
   pub400 asking for a named device the account may use.
5. **The cursor without an Insert Cursor order.** Resolved from the
   same trace: pub400's sign-on sends no `IC`; ACS shows the cursor in
   the first input field. The terminal rests the cursor in the first
   non-bypass field when the keyboard opens.
6. **Which keys carry field data.** Open, assumed: Enter, F1–F24, Roll
   Up/Down and Auto Enter send the modified fields; Help, Clear, Print
   and Record Backspace send only the cursor and AID; System Request,
   Attention and Test Request are header flags. A probe with a DDS
   display file that reads after each key would settle it on a system
   where objects may be created.
7. **Start of Header and the format table.** Assumed: `SOH` clears the
   format table, locks the keyboard and homes the cursor (a full
   screen write begins with it; partial updates do not send it).
   Consistent with every pub400 screen seen so far.
8. **Password tokens for auto-signon.** Resolved by design: the
   profile keeps the RFC 4777 DES and SHA-1 tokens of its password
   beside the Argon2id hashes; SHA-1 verified against pub400's server
   (our client side), the server side against our own client.
