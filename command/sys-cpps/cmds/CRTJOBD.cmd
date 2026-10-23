/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTJOBD: create a job description. */
             CMD        PROMPT('Create Job Description') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CRTJOBD)
             PARM       KWD(JOBD) TYPE(Q1) MIN(1) PROMPT('Job description')
             PARM       KWD(JOBQ) TYPE(Q2) DFT(QBATCH) PROMPT('Job queue')
             PARM       KWD(JOBPTY) TYPE(*DEC) LEN(1) DFT(5) RANGE(1 9) +
                          PROMPT('Job priority (on JOBQ)')
             PARM       KWD(OUTPTY) TYPE(*INT2) DFT(5) RANGE(1 9) +
                          PROMPT('Output priority (on OUTQ)')
             PARM       KWD(PRTDEV) TYPE(*NAME) LEN(10) DFT(*USRPRF) +
                          SPCVAL((*USRPRF) (*SYSVAL) (*WRKSTN)) +
                          PROMPT('Print device')
             PARM       KWD(OUTQ) TYPE(Q2) DFT(*USRPRF) +
                          SNGVAL((*USRPRF) (*DEV) (*WRKSTN)) +
                          PROMPT('Output queue')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(USER) TYPE(*NAME) LEN(10) DFT(*RQD) +
                          SPCVAL((*RQD)) PMTCTL(*PMTRQS) PROMPT('User')
             PARM       KWD(PRTTXT) TYPE(*CHAR) LEN(30) DFT(*SYSVAL) +
                          SPCVAL((*SYSVAL) (*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Print text')
             PARM       KWD(ACGCDE) TYPE(*CHAR) LEN(15) DFT(*USRPRF) +
                          SPCVAL((*USRPRF) (*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Accounting code')
             PARM       KWD(RTGDTA) TYPE(*CHAR) LEN(80) DFT(QCMDI) +
                          SPCVAL((*RQSDTA)) PMTCTL(*PMTRQS) +
                          PROMPT('Routing data')
             PARM       KWD(RQSDTA) TYPE(*CHAR) LEN(256) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Request data')
             PARM       KWD(SYNTAX) TYPE(*INT2) DFT(*NOCHK) RANGE(0 99) +
                          SPCVAL((*NOCHK -1)) PMTCTL(*PMTRQS) +
                          PROMPT('CL syntax check severity')
             PARM       KWD(INLLIBL) TYPE(*NAME) LEN(10) MAX(250) +
                          DFT(*SYSVAL) SNGVAL((*SYSVAL) (*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Initial library list')
             PARM       KWD(ENDSEV) TYPE(*INT2) DFT(30) RANGE(0 99) +
                          PMTCTL(*PMTRQS) PROMPT('End severity')
             PARM       KWD(LOG) TYPE(E1) DFT((4 0 *NOLIST)) +
                          PMTCTL(*PMTRQS) PROMPT('Message logging')
             PARM       KWD(LOGCLPGM) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Log CL program commands')
             PARM       KWD(LOGOUTPUT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *JOBLOGSVR *JOBEND +
                          *PND) PMTCTL(*PMTRQS) PROMPT('Log output')
             PARM       KWD(INQMSGRPY) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*RQD) VALUES(*RQD *DFT *SYSRPYL) +
                          PMTCTL(*PMTRQS) PROMPT('Inquiry message reply')
             PARM       KWD(HOLD) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*NO) +
                          VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Hold on job queue')
             PARM       KWD(DATE) TYPE(*DATE) DFT(*SYSVAL) +
                          SPCVAL((*SYSVAL)) PMTCTL(*PMTRQS) +
                          PROMPT('Job date')
             PARM       KWD(SWS) TYPE(*CHAR) LEN(8) DFT(00000000) +
                          PMTCTL(*PMTRQS) PROMPT('Job switches')
             PARM       KWD(JOBMSGQMX) TYPE(*INT2) DFT(*SYSVAL) +
                          RANGE(2 64) SPCVAL((*SYSVAL 0)) PMTCTL(*PMTRQS) +
                          PROMPT('Job message queue maximum')
             PARM       KWD(JOBMSGQFL) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *NOWRAP *WRAP +
                          *PRTWRAP) PMTCTL(*PMTRQS) +
                          PROMPT('Job message queue full action')
             PARM       KWD(DEVRCYACN) TYPE(*CHAR) LEN(13) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *MSG *DSCMSG +
                          *DSCENDRQS *ENDJOB *ENDJOBNOLIST) +
                          PMTCTL(*PMTRQS) PROMPT('Device recovery action')
             PARM       KWD(TSEPOOL) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *NONE *BASE) +
                          PMTCTL(*PMTRQS) PROMPT('Time slice end pool')
             PARM       KWD(SPLFACN) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *KEEP *DETACH) +
                          PMTCTL(*PMTRQS) PROMPT('Spooled file action')
             PARM       KWD(DDMCNV) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*KEEP) VALUES(*KEEP *DROP) PMTCTL(*PMTRQS) +
                          PROMPT('DDM conversation')
             PARM       KWD(ALWMLTTHD) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Allow multiple threads')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
             PARM       KWD(INLASPGRP) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Initial ASP group')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT2) DFT(4) RANGE(0 4) +
                          PROMPT('Level')
                        ELEM       TYPE(*INT2) DFT(0) RANGE(0 99) +
                          PROMPT('Severity')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NOLIST) VALUES(*MSG *SECLVL *NOLIST) +
                          PROMPT('Text')
