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
8. **The printer data stream.** Open. The server sends each SCS chunk
   as a GDS record (opcode 0) and waits for a record back; RFC 2877's
   exact printer header and the client's response codes could not be
   checked (no printer client reachable). Needs an ACS printer session
   against QS/400, or a trace of one against a real system.
9. **Password tokens for auto-signon.** Resolved by design: the
   profile keeps the RFC 4777 DES and SHA-1 tokens of its password
   beside the Argon2id hashes; SHA-1 verified against pub400's server
   (our client side), the server side against our own client.
10. **Message IDs the second wave guessed.** Settled 2026-10-23 from
    the message IDs and first-level texts of pub400's `QCPFMSG` (read
    with `QSYS2.MESSAGE_FILE_DATA` over JDBC; V7R1M0 lacks that
    view). `CPF1907` (a request ended from System Request) is IBM's.
    A panel group or panel that cannot be shown is `CPF6A12` (unable
    to open panel group); the drivers had used `CPF6A98`, which is
    QUIGETLE's "entry not found in list". The group-job IDs were
    renumbered to IBM's meanings: a name that is not valid `CPF1313`,
    leaving the group while other jobs are in it `CPF1306`, a group
    job name already used `CPF1307`, a transfer that cannot be done
    (no such group job, or the job itself) `CPF1310`, not a group job
    `CPF1311`; `CPF1309` stays for a request the group cannot
    complete. Our texts are our own.
11. **`CPF2222` for a missing special authority.** Open, outside this
    phase. Phase 6 refuses a missing special authority (STRSST
    without `*SERVICE` among them) with `CPF2222`; IBM's `CPF2222` is
    a storage-limit message. The command-level refusal IBM gives
    varies by command and was not settled; a later phase picks the
    IDs per command from the oracle.
12. **SST's audit code.** Open. An SST sign-on is audited as `SS`,
    an entry type of ours beside DST's `DS`; IBM's code for it was
    not found.
