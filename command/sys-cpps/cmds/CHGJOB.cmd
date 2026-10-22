/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGJOB: change the attributes of a job. */
             CMD        PROMPT('Change Job') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CHGJOB)
             PARM       KWD(JOB) TYPE(QJ) DFT(*) PROMPT('Job name')
             PARM       KWD(JOBPTY) TYPE(*INT2) DFT(*SAME) RANGE(0 9) +
                          SPCVAL((*SAME -1)) PROMPT('Job priority (on JOBQ)')
             PARM       KWD(OUTPTY) TYPE(*INT2) DFT(*SAME) RANGE(1 9) +
                          SPCVAL((*SAME -1)) +
                          PROMPT('Output priority (on OUTQ)')
             PARM       KWD(PRTDEV) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*USRPRF) (*SYSVAL) (*WRKSTN)) +
                          PROMPT('Print device')
             PARM       KWD(OUTQ) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*USRPRF) (*DEV) (*WRKSTN)) +
                          PROMPT('Output queue')
             PARM       KWD(RUNPTY) TYPE(*INT2) DFT(*SAME) RANGE(1 99) +
                          SPCVAL((*SAME -1)) PROMPT('Run priority')
             PARM       KWD(JOBQ) TYPE(Q1) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Job queue')
             PARM       KWD(PRTTXT) TYPE(*CHAR) LEN(30) DFT(*SAME) +
                          SPCVAL((*SAME) (*SYSVAL) (*BLANK ' ')) +
                          PROMPT('Print text')
             PARM       KWD(LOG) TYPE(E1) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Message logging')
             PARM       KWD(LOGCLPGM) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Log CL program commands')
             PARM       KWD(LOGOUTPUT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *JOBLOGSVR +
                          *JOBEND *PND) PROMPT('Log output')
             PARM       KWD(JOBMSGQFL) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *NOWRAP *WRAP +
                          *PRTWRAP) PROMPT('Job message queue full action')
             PARM       KWD(INQMSGRPY) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *RQD *DFT *SYSRPYL) +
                          PROMPT('Inquiry message reply')
             PARM       KWD(BRKMSG) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NORMAL *NOTIFY *HOLD) +
                          PROMPT('Break message handling')
             PARM       KWD(STSMSG) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *USRPRF *SYSVAL *NONE +
                          *NORMAL) PROMPT('Status message')
             PARM       KWD(DDMCNV) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *KEEP *DROP) +
                          PROMPT('DDM conversation')
             PARM       KWD(SCDDATE) TYPE(*DATE) DFT(*SAME) +
                          SPCVAL((*SAME) (*CURRENT) (*MONTHSTR) (*MONTHEND) +
                          (*MON) (*TUE) (*WED) (*THU) (*FRI) (*SAT) (*SUN)) +
                          PROMPT('Schedule date')
             PARM       KWD(SCDTIME) TYPE(*TIME) DFT(*SAME) +
                          SPCVAL((*SAME) (*CURRENT)) PROMPT('Schedule time')
             PARM       KWD(DATE) TYPE(*DATE) DFT(*SAME) SPCVAL((*SAME)) +
                          PROMPT('Job date')
             PARM       KWD(DATFMT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *YMD *MDY *DMY +
                          *JUL) PROMPT('Date format')
             PARM       KWD(DATSEP) TYPE(*CHAR) LEN(1) DFT(*SAME) +
                          SPCVAL((*SAME) (*SYSVAL) (*BLANK ' ')) +
                          PROMPT('Date separator')
             PARM       KWD(TIMSEP) TYPE(*CHAR) LEN(1) DFT(*SAME) +
                          SPCVAL((*SAME) (*SYSVAL) (*BLANK ' ')) +
                          PROMPT('Time separator')
             PARM       KWD(DECFMT) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*SAME) VALUES(I J) +
                          SPCVAL((*SAME) (*SYSVAL) (*BLANK ' ')) +
                          PROMPT('Decimal format')
             PARM       KWD(SWS) TYPE(*CHAR) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Job switches')
             PARM       KWD(TIMESLICE) TYPE(*INT4) DFT(*SAME) +
                          RANGE(8 9999999) SPCVAL((*SAME -1)) +
                          PROMPT('Time slice')
             PARM       KWD(PURGE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Eligible for purge')
             PARM       KWD(DFTWAIT) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 9999999) SPCVAL((*SAME -1) (*NOMAX 0)) +
                          PROMPT('Default wait time')
             PARM       KWD(DEVRCYACN) TYPE(*CHAR) LEN(13) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *MSG *DSCMSG +
                          *DSCENDRQS *ENDJOB *ENDJOBNOLIST) +
                          PROMPT('Device recovery action')
             PARM       KWD(TSEPOOL) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *NONE *BASE) +
                          PROMPT('Time slice end pool')
             PARM       KWD(PRTKEYFMT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *NONE *PRTBDR +
                          *PRTHDR *PRTALL) PROMPT('Print key format')
             PARM       KWD(SRTSEQ) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*USRPRF) (*SYSVAL) (*HEX) +
                          (*LANGIDUNQ) (*LANGIDSHR)) PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*CHAR) LEN(3) DFT(*SAME) +
                          SPCVAL((*SAME) (*USRPRF) (*SYSVAL)) +
                          PROMPT('Language ID')
             PARM       KWD(CNTRYID) TYPE(*CHAR) LEN(2) DFT(*SAME) +
                          SPCVAL((*SAME) (*USRPRF) (*SYSVAL)) +
                          PROMPT('Country or region ID')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*SAME) RANGE(1 65535) +
                          SPCVAL((*SAME -3) (*USRPRF -1) (*SYSVAL -2) +
                          (*HEX 65535)) PROMPT('Coded character set ID')
             PARM       KWD(CHRIDCTL) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *USRPRF *SYSVAL *DEVD +
                          *JOBCCSID) PROMPT('Character identifier control')
             PARM       KWD(SPLFACN) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *KEEP *DETACH) +
                          PROMPT('Spooled file action')
             PARM       KWD(DUPJOBOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SELECT) VALUES(*SELECT *MSG) +
                          PROMPT('Duplicate job option')
             QJ:        QUAL       TYPE(*NAME) LEN(10) SPCVAL((*))
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) RANGE(000000 999999) +
                          FULL(*YES) PROMPT('Number')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT2) DFT(*SAME) RANGE(0 4) +
                          SPCVAL((*SAME -1)) PROMPT('Level')
                        ELEM       TYPE(*INT2) DFT(*SAME) RANGE(0 99) +
                          SPCVAL((*SAME -1)) PROMPT('Severity')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *MSG *SECLVL *NOLIST) +
                          PROMPT('Text')
