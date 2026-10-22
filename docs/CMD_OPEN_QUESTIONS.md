# Commands and CL: open questions

Readings a real IBM i would settle. Each names what this system does
now and what to check.

1. **Prompt displays.** The goldens under `command/cmd/tests/golden/`
   and `command/sys-cpps/tests/golden/` are our renders: label column
   2, dots to column 33, the `>` marker in 35, the field in 37, the
   choices from 51, 16 parameter rows per page. IBM's exact columns,
   the choice texts for ranges and special values, and where a long
   prompt text wraps are unverified. A capture of F4 on CRTPF, CRTLIB,
   SBMJOB and CHGJOB on the real box would fix them.

2. **The analyzer kit.** `tools/conformance/cmd/expected.txt` is our
   outcome for 797 strings; `ORACLE.CLLE` gathers the real box's. The
   identifiers we send for each refusal (CPD0012 bad keyword, CPD0015
   required, CPD0016 too many values, CPD0084 bad value, CPD0093 out
   of range) follow IBM's documented ones, but which diagnostic IBM
   sends first when several apply, and whether CPD0014 or CPD0015
   names a missing required parameter, is unverified.

3. **Three-part qualified names.** IBM spells a job name
   `number/user/job` with slashes and a file member `LIB/FILE MBR` with
   a blank, both `QUAL` groups of three. The analyzer renders the parts
   past the second after a blank (`JOB(ABC/* ABC)`), so a job name
   rendered back differs from how it was typed. Whether IBM's QUAL
   definitions carry a spelling flag, or the parser keeps the form it
   read, is unverified.

4. **`EXPR(*YES)`.** Our shipped definitions leave it out, so the
   analyzer takes an expression on any character, numeric, logical or
   name-typed scalar. IBM's definitions mark it per parameter; a
   parameter where IBM refuses `'a' *CAT 'b'` at the command line is
   unknown to us.

5. **Return variable lengths.** IBM's rule for a `RTNVAL(*YES)`
   character parameter is "at least the length" for fixed results and
   any length for `VARY` results. Ours: at least the length, unless
   the parameter is longer than 100. `RTVSYSVAL RTNVAR` is declared
   `LEN(1)` so a variable of the system value's length passes; IBM's
   definition may differ.

6. **`MONMSG CMPDTA`.** Accepted and not compared (warning CPD0790).
   IBM compares the first characters of the message data; where the
   data starts for the common messages (CPF9801's object name at
   offset 0?) decides which captures match.

7. **The direct path's block.** When a CL program calls a system
   command's processing program directly, IBM passes exactly the
   layout `docs/CMD.md` §4 describes for every type; whether IBM
   passes `*INT2` as a 2-byte or a 4-byte field in the block, and
   whether a `PASSATR` byte precedes a list, is read from the
   documentation, not a dump.

8. **Command Entry.** Our panel: the previous commands and messages on
   rows 5 to 18, `Bottom`/`More...` at row 18 column 73, the command
   line on rows 20 to 22, the keys on 23 and 24, F9 cycling the last
   25 commands, F10 adding second-level text under each message, F11
   wrapping long lines. IBM's row for the roll indicator, the exact
   key words and what F11 shows are unverified against a capture.

9. **`QCMDEXC` from a program at `*PRV`.** A message sent to `*PRV`
   by the first program of a job goes to the external queue, and an
   escape there ends the job's call with the exception pending. IBM
   ends the request and shows the message at Command Entry; the
   inquiry a batch job gets instead is unverified.

10. **The listing.** Our listing's column layout (statement number,
    nesting, source; the cross-reference columns) imitates IBM's shape
    with our words. A real `CRTBNDCL` listing would show the line
    widths and whether the cross-reference lists subroutines.
