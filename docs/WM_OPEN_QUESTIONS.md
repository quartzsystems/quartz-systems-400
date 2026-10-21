# Work management open questions

Behaviour of jobs, queues and subsystems that the IBM documentation
leaves open, or that QS/400 does differently for now, with the reading
implemented and the test on the real machine that settles it. An entry
closes when the machine's answer is recorded here and the code agrees.

## Jobs

1. **Job number of `SCPF`.** Implemented: `000000`, and numbers handed
   out from 1 up, wrapping after 999999 and skipping active jobs. The
   machine shows `SCPF` as `000000`; whether the counter skips numbers
   of jobs still on output queues (ended, spooled files kept) is not
   documented. Test: on the machine, `WRKUSRJOB *ALL *OUTQ`, note the
   lowest number held, and watch whether `SBMJOB` ever reuses it after
   a wrap. Until spooled files exist here, only active numbers are
   skipped.

2. **What `DSPJOBLOG` shows of an active job.** Implemented: the
   entries `LOG(level severity *)` keeps, with level 0 read as 1 so the
   display is never empty of messages; the printed log applies the
   level exactly. Test: `CHGJOB LOG(0 99 *NOLIST)`, send a message to
   `*EXT`, `DSPJOBLOG`: whether the machine shows it.

3. **Severity of a request message in the job log.** Implemented: 00.
   The machine's `QPJOBLOG` shows requests as `Request ... 00`; this
   matches every log seen, but is not documented.

## Messages

4. **Break delivery inside a running program.** Implemented: a job
   takes its break messages at its wait points (data queue and message
   waits, the console's idle wait) and when each request returns. The
   machine interrupts a program at the next machine-instruction
   boundary. Here that needs a check at every statement of every
   program, which generated code has only in its debug variant; the
   cost of adding it everywhere is not yet measured. Test on the
   machine: a CL program looping `CHGVAR` for ten seconds with no
   waits, a break message sent meanwhile: it is shown during the loop.
   QS/400 shows it when the call ends.

5. **`MSGQFULL(*WRAP)` and unanswered inquiries.** Implemented: wrap
   removes the oldest messages that are not inquiries waiting for a
   reply; a queue full of unanswered inquiries rejects the new message
   with `CPF2460`. Test: a queue of `SIZE(1 1 1) MSGQFULL(*WRAP)`
   filled with inquiries, then one more message.
