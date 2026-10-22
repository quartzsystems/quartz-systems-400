/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SBMJOB: submit a command to run as a batch job. */
             CMD        PROMPT('Submit Job') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(SBMJOB)
             PARM       KWD(CMD) TYPE(*CMDSTR) LEN(3000) +
                          PROMPT('Command to run')
             PARM       KWD(JOB) TYPE(*NAME) LEN(10) DFT(*JOBD) +
                          SPCVAL((*JOBD)) PROMPT('Job name')
             PARM       KWD(JOBD) TYPE(Q1) DFT(*USRPRF) +
                          SNGVAL((*USRPRF)) PROMPT('Job description')
             PARM       KWD(JOBQ) TYPE(Q1) DFT(*JOBD) SNGVAL((*JOBD)) +
                          PROMPT('Job queue')
             PARM       KWD(JOBPTY) TYPE(*INT2) DFT(*JOBD) RANGE(1 9) +
                          SPCVAL((*JOBD 0)) PROMPT('Job priority (on JOBQ)')
             PARM       KWD(OUTPTY) TYPE(*INT2) DFT(*JOBD) RANGE(1 9) +
                          SPCVAL((*JOBD 0)) +
                          PROMPT('Output priority (on OUTQ)')
             PARM       KWD(PRTDEV) TYPE(*NAME) LEN(10) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*USRPRF) (*SYSVAL) (*JOBD)) +
                          PROMPT('Print device')
             PARM       KWD(OUTQ) TYPE(Q1) DFT(*CURRENT) +
                          SNGVAL((*CURRENT) (*USRPRF) (*DEV) (*JOBD)) +
                          PROMPT('Output queue')
             PARM       KWD(USER) TYPE(*NAME) LEN(10) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*JOBD)) PROMPT('User')
             PARM       KWD(PRTTXT) TYPE(*CHAR) LEN(30) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*JOBD) (*SYSVAL) (*BLANK ' ')) +
                          PROMPT('Print text')
             PARM       KWD(RTGDTA) TYPE(*CHAR) LEN(80) DFT(QCMDB) +
                          SPCVAL((*JOBD) (*RQSDTA)) PMTCTL(*PMTRQS) +
                          PROMPT('Routing data')
             PARM       KWD(RQSDTA) TYPE(*CHAR) LEN(3000) DFT(*CMD) +
                          SPCVAL((*CMD) (*NONE) (*JOBD)) PMTCTL(*PMTRQS) +
                          PROMPT('Request data')
             PARM       KWD(SYSLIBL) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*CURRENT) VALUES(*CURRENT *SYSVAL) +
                          PMTCTL(*PMTRQS) PROMPT('System library list')
             PARM       KWD(CURLIB) TYPE(*NAME) LEN(10) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*USRPRF) (*CRTDFT)) +
                          PMTCTL(*PMTRQS) PROMPT('Current library')
             PARM       KWD(INLLIBL) TYPE(*NAME) LEN(10) MAX(250) +
                          DFT(*CURRENT) SNGVAL((*CURRENT) (*JOBD) +
                          (*SYSVAL) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Initial library list')
             PARM       KWD(LOG) TYPE(E1) DFT(*JOBD) SNGVAL((*JOBD)) +
                          PMTCTL(*PMTRQS) PROMPT('Message logging')
             PARM       KWD(LOGCLPGM) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*JOBD) VALUES(*JOBD *NO *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Log CL program commands')
             PARM       KWD(LOGOUTPUT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SYSVAL) VALUES(*SYSVAL *JOBD *JOBLOGSVR +
                          *JOBEND *PND) PMTCTL(*PMTRQS) +
                          PROMPT('Log output')
             PARM       KWD(JOBMSGQMX) TYPE(*INT2) DFT(*JOBD) +
                          RANGE(2 64) SPCVAL((*JOBD 0) (*SYSVAL -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Job message queue maximum')
             PARM       KWD(JOBMSGQFL) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*JOBD) VALUES(*JOBD *SYSVAL *NOWRAP *WRAP +
                          *PRTWRAP) PMTCTL(*PMTRQS) +
                          PROMPT('Job message queue full action')
             PARM       KWD(INQMSGRPY) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*JOBD) VALUES(*JOBD *RQD *DFT *SYSRPYL) +
                          PMTCTL(*PMTRQS) PROMPT('Inquiry message reply')
             PARM       KWD(HOLD) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*JOBD) VALUES(*JOBD *NO *YES) +
                          PMTCTL(*PMTRQS) PROMPT('Hold on job queue')
             PARM       KWD(SCDDATE) TYPE(*DATE) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*MONTHSTR) (*MONTHEND) (*MON) +
                          (*TUE) (*WED) (*THU) (*FRI) (*SAT) (*SUN)) +
                          PMTCTL(*PMTRQS) PROMPT('Schedule date')
             PARM       KWD(SCDTIME) TYPE(*TIME) DFT(*CURRENT) +
                          SPCVAL((*CURRENT)) PMTCTL(*PMTRQS) +
                          PROMPT('Schedule time')
             PARM       KWD(DATE) TYPE(*DATE) DFT(*JOBD) SPCVAL((*JOBD)) +
                          PMTCTL(*PMTRQS) PROMPT('Job date')
             PARM       KWD(SWS) TYPE(*CHAR) LEN(8) DFT(*JOBD) +
                          SPCVAL((*JOBD)) PMTCTL(*PMTRQS) +
                          PROMPT('Job switches')
             PARM       KWD(DSPSBMJOB) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Allow display by WRKSBMJOB')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*USRPRF) +
                          SNGVAL((*USRPRF) (*WRKSTN) (*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Message queue')
             PARM       KWD(SRTSEQ) TYPE(Q1) DFT(*CURRENT) +
                          SNGVAL((*CURRENT) (*USRPRF) (*SYSVAL) (*HEX) +
                          (*LANGIDUNQ) (*LANGIDSHR)) PMTCTL(*PMTRQS) +
                          PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*CHAR) LEN(3) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*USRPRF) (*SYSVAL)) +
                          PMTCTL(*PMTRQS) PROMPT('Language ID')
             PARM       KWD(CNTRYID) TYPE(*CHAR) LEN(2) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*USRPRF) (*SYSVAL)) +
                          PMTCTL(*PMTRQS) PROMPT('Country or region ID')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*CURRENT) +
                          RANGE(1 65535) SPCVAL((*CURRENT 0) (*USRPRF -1) +
                          (*SYSVAL -2) (*HEX 65535)) PMTCTL(*PMTRQS) +
                          PROMPT('Coded character set ID')
             PARM       KWD(CPYENVVAR) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Copy environment variables')
             PARM       KWD(ALWMLTTHD) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Allow multiple threads')
             PARM       KWD(SPLFACN) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*JOBD) VALUES(*JOBD *SYSVAL *KEEP *DETACH) +
                          PMTCTL(*PMTRQS) PROMPT('Spooled file action')
             PARM       KWD(DDMCNV) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*JOBD) VALUES(*JOBD *KEEP *DROP) +
                          PMTCTL(*PMTRQS) PROMPT('DDM conversation')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT2) DFT(4) RANGE(0 4) +
                          PROMPT('Level')
                        ELEM       TYPE(*INT2) DFT(0) RANGE(0 99) +
                          PROMPT('Severity')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NOLIST) VALUES(*MSG *SECLVL *NOLIST) +
                          PROMPT('Text')
