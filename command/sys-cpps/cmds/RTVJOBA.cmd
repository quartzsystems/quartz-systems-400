/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RTVJOBA: put the attributes of the current job into CL variables. */
             CMD        PROMPT('Retrieve Job Attributes') +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(RTVJOBA)
             PARM       KWD(JOB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for JOB (10)')
             PARM       KWD(USER) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for USER (10)')
             PARM       KWD(NBR) TYPE(*CHAR) LEN(6) RTNVAL(*YES) +
                          PROMPT('CL var for NBR (6)')
             PARM       KWD(LOGLVL) TYPE(*DEC) LEN(1 0) RTNVAL(*YES) +
                          PROMPT('CL var for LOGLVL (1 0)')
             PARM       KWD(LOGSEV) TYPE(*DEC) LEN(2 0) RTNVAL(*YES) +
                          PROMPT('CL var for LOGSEV (2 0)')
             PARM       KWD(LOGCLPGM) TYPE(*CHAR) LEN(4) RTNVAL(*YES) +
                          PROMPT('CL var for LOGCLPGM (4)')
             PARM       KWD(OUTQ) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for OUTQ (10)')
             PARM       KWD(OUTQLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for OUTQLIB (10)')
             PARM       KWD(PRTDEV) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for PRTDEV (10)')
             PARM       KWD(INQMSGRPY) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for INQMSGRPY (10)')
             PARM       KWD(BRKMSG) TYPE(*CHAR) LEN(7) RTNVAL(*YES) +
                          PROMPT('CL var for BRKMSG (7)')
             PARM       KWD(STSMSG) TYPE(*CHAR) LEN(7) RTNVAL(*YES) +
                          PROMPT('CL var for STSMSG (7)')
             PARM       KWD(DEVRCYACN) TYPE(*CHAR) LEN(13) RTNVAL(*YES) +
                          PROMPT('CL var for DEVRCYACN (13)')
             PARM       KWD(TSEPOOL) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for TSEPOOL (10)')
             PARM       KWD(PRTTXT) TYPE(*CHAR) LEN(30) RTNVAL(*YES) +
                          PROMPT('CL var for PRTTXT (30)')
             PARM       KWD(PRTKEYFMT) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for PRTKEYFMT (10)')
             PARM       KWD(SRTSEQ) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SRTSEQ (10)')
             PARM       KWD(SRTSEQLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SRTSEQLIB (10)')
             PARM       KWD(LANGID) TYPE(*CHAR) LEN(3) RTNVAL(*YES) +
                          PROMPT('CL var for LANGID (3)')
             PARM       KWD(CNTRYID) TYPE(*CHAR) LEN(2) RTNVAL(*YES) +
                          PROMPT('CL var for CNTRYID (2)')
             PARM       KWD(CCSID) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('CL var for CCSID (5 0)')
             PARM       KWD(DFTCCSID) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('CL var for DFTCCSID (5 0)')
             PARM       KWD(CHRIDCTL) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for CHRIDCTL (10)')
             PARM       KWD(SWS) TYPE(*CHAR) LEN(8) RTNVAL(*YES) +
                          PROMPT('CL var for SWS (8)')
             PARM       KWD(ACGCDE) TYPE(*CHAR) LEN(15) RTNVAL(*YES) +
                          PROMPT('CL var for ACGCDE (15)')
             PARM       KWD(DATE) TYPE(*CHAR) LEN(6) RTNVAL(*YES) +
                          PROMPT('CL var for DATE (6)')
             PARM       KWD(CURLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for CURLIB (10)')
             PARM       KWD(SYSLIBL) TYPE(*CHAR) LEN(165) RTNVAL(*YES) +
                          PROMPT('CL var for SYSLIBL (165)')
             PARM       KWD(USRLIBL) TYPE(*CHAR) LEN(2750) RTNVAL(*YES) +
                          PROMPT('CL var for USRLIBL (2750)')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(1) RTNVAL(*YES) +
                          PROMPT('CL var for TYPE (1)')
             PARM       KWD(RUNPTY) TYPE(*DEC) LEN(2 0) RTNVAL(*YES) +
                          PROMPT('CL var for RUNPTY (2 0)')
             PARM       KWD(TIMESLICE) TYPE(*DEC) LEN(7 0) RTNVAL(*YES) +
                          PROMPT('CL var for TIMESLICE (7 0)')
             PARM       KWD(PURGE) TYPE(*CHAR) LEN(4) RTNVAL(*YES) +
                          PROMPT('CL var for PURGE (4)')
             PARM       KWD(DFTWAIT) TYPE(*DEC) LEN(7 0) RTNVAL(*YES) +
                          PROMPT('CL var for DFTWAIT (7 0)')
             PARM       KWD(DDMCNV) TYPE(*CHAR) LEN(5) RTNVAL(*YES) +
                          PROMPT('CL var for DDMCNV (5)')
             PARM       KWD(DATFMT) TYPE(*CHAR) LEN(4) RTNVAL(*YES) +
                          PROMPT('CL var for DATFMT (4)')
             PARM       KWD(DATSEP) TYPE(*CHAR) LEN(1) RTNVAL(*YES) +
                          PROMPT('CL var for DATSEP (1)')
             PARM       KWD(TIMSEP) TYPE(*CHAR) LEN(1) RTNVAL(*YES) +
                          PROMPT('CL var for TIMSEP (1)')
             PARM       KWD(DECFMT) TYPE(*CHAR) LEN(1) RTNVAL(*YES) +
                          PROMPT('CL var for DECFMT (1)')
             PARM       KWD(ENDSTS) TYPE(*CHAR) LEN(1) RTNVAL(*YES) +
                          PROMPT('CL var for ENDSTS (1)')
             PARM       KWD(SBMMSGQ) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SBMMSGQ (10)')
             PARM       KWD(SBMMSGQLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SBMMSGQLIB (10)')
             PARM       KWD(SBSD) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SBSD (10)')
             PARM       KWD(SBSDLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SBSDLIB (10)')
             PARM       KWD(JOBD) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for JOBD (10)')
             PARM       KWD(JOBDLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for JOBDLIB (10)')
             PARM       KWD(JOBQ) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for JOBQ (10)')
             PARM       KWD(JOBQLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for JOBQLIB (10)')
             PARM       KWD(JOBPTY) TYPE(*DEC) LEN(1 0) RTNVAL(*YES) +
                          PROMPT('CL var for JOBPTY (1 0)')
             PARM       KWD(OUTPTY) TYPE(*DEC) LEN(1 0) RTNVAL(*YES) +
                          PROMPT('CL var for OUTPTY (1 0)')
             PARM       KWD(SPLFACN) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SPLFACN (10)')
             PARM       KWD(LOGOUTPUT) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for LOGOUTPUT (10)')
             PARM       KWD(JOBMSGQMX) TYPE(*DEC) LEN(2 0) RTNVAL(*YES) +
                          PROMPT('CL var for JOBMSGQMX (2 0)')
             PARM       KWD(JOBMSGQFL) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for JOBMSGQFL (10)')
             PARM       KWD(CURUSER) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for CURUSER (10)')
             PARM       KWD(TIMEZONE) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for TIMEZONE (10)')
