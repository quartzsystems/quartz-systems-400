/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGJOBD: change a job description. */
             CMD        PROMPT('Change Job Description') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CHGJOBD)
             PARM       KWD(JOBD) TYPE(Q1) MIN(1) PROMPT('Job description')
             PARM       KWD(JOBQ) TYPE(Q2) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Job queue')
             PARM       KWD(JOBPTY) TYPE(*INT2) DFT(*SAME) RANGE(1 9) +
                          SPCVAL((*SAME -1)) PROMPT('Job priority (on JOBQ)')
             PARM       KWD(OUTPTY) TYPE(*INT2) DFT(*SAME) RANGE(1 9) +
                          SPCVAL((*SAME -1)) +
                          PROMPT('Output priority (on OUTQ)')
             PARM       KWD(PRTDEV) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*USRPRF) (*SYSVAL) (*WRKSTN)) +
                          PROMPT('Print device')
             PARM       KWD(OUTQ) TYPE(Q2) DFT(*SAME) +
                          SNGVAL((*SAME) (*USRPRF) (*DEV) (*WRKSTN)) +
                          PROMPT('Output queue')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) PROMPT('Description')
             PARM       KWD(USER) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*RQD)) PROMPT('User')
             PARM       KWD(PRTTXT) TYPE(*CHAR) LEN(30) DFT(*SAME) +
                          SPCVAL((*SAME) (*SYSVAL) (*BLANK ' ')) +
                          PROMPT('Print text')
             PARM       KWD(ACGCDE) TYPE(*CHAR) LEN(15) DFT(*SAME) +
                          SPCVAL((*SAME) (*USRPRF) (*BLANK ' ')) +
                          PROMPT('Accounting code')
             PARM       KWD(RTGDTA) TYPE(*CHAR) LEN(80) DFT(*SAME) +
                          SPCVAL((*SAME) (*RQSDTA)) PROMPT('Routing data')
             PARM       KWD(RQSDTA) TYPE(*CHAR) LEN(256) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) PROMPT('Request data')
             PARM       KWD(SYNTAX) TYPE(*INT2) DFT(*SAME) RANGE(0 99) +
                          SPCVAL((*SAME -2) (*NOCHK -1)) +
                          PROMPT('CL syntax check severity')
             PARM       KWD(INLLIBL) TYPE(*NAME) LEN(10) MAX(250) +
                          DFT(*SAME) SNGVAL((*SAME) (*SYSVAL) (*NONE)) +
                          PROMPT('Initial library list')
             PARM       KWD(ENDSEV) TYPE(*INT2) DFT(*SAME) RANGE(0 99) +
                          SPCVAL((*SAME -1)) PROMPT('End severity')
             PARM       KWD(LOG) TYPE(E1) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Message logging')
             PARM       KWD(LOGCLPGM) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Log CL program commands')
             PARM       KWD(LOGOUTPUT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *JOBLOGSVR +
                          *JOBEND *PND) PROMPT('Log output')
             PARM       KWD(INQMSGRPY) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *RQD *DFT *SYSRPYL) +
                          PROMPT('Inquiry message reply')
             PARM       KWD(HOLD) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Hold on job queue')
             PARM       KWD(DATE) TYPE(*DATE) DFT(*SAME) +
                          SPCVAL((*SAME) (*SYSVAL)) PROMPT('Job date')
             PARM       KWD(SWS) TYPE(*CHAR) LEN(8) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Job switches')
             PARM       KWD(JOBMSGQMX) TYPE(*INT2) DFT(*SAME) +
                          RANGE(2 64) SPCVAL((*SAME -1) (*SYSVAL 0)) +
                          PROMPT('Job message queue maximum')
             PARM       KWD(JOBMSGQFL) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *NOWRAP *WRAP +
                          *PRTWRAP) PROMPT('Job message queue full action')
             PARM       KWD(DEVRCYACN) TYPE(*CHAR) LEN(13) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *MSG *DSCMSG +
                          *DSCENDRQS *ENDJOB *ENDJOBNOLIST) +
                          PROMPT('Device recovery action')
             PARM       KWD(TSEPOOL) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *NONE *BASE) +
                          PROMPT('Time slice end pool')
             PARM       KWD(SPLFACN) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SYSVAL *KEEP *DETACH) +
                          PROMPT('Spooled file action')
             PARM       KWD(DDMCNV) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *KEEP *DROP) +
                          PROMPT('DDM conversation')
             PARM       KWD(ALWMLTTHD) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Allow multiple threads')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT2) DFT(*SAME) RANGE(0 4) +
                          SPCVAL((*SAME -1)) PROMPT('Level')
                        ELEM       TYPE(*INT2) DFT(*SAME) RANGE(0 99) +
                          SPCVAL((*SAME -1)) PROMPT('Severity')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *MSG *SECLVL *NOLIST) +
                          PROMPT('Text')
