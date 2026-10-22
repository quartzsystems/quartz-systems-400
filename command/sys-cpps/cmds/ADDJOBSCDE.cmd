/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDJOBSCDE: add an entry to the job schedule. */
             CMD        PROMPT('Add Job Schedule Entry') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(ADDJOBSCDE)
             PARM       KWD(JOB) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*JOBD)) PROMPT('Job name')
             PARM       KWD(CMD) TYPE(*CMDSTR) LEN(512) MIN(1) +
                          PROMPT('Command to run')
             PARM       KWD(FRQ) TYPE(*CHAR) LEN(8) RSTD(*YES) MIN(1) +
                          VALUES(*ONCE *WEEKLY *MONTHLY) PROMPT('Frequency')
             PARM       KWD(SCDDATE) TYPE(*DATE) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*MONTHSTR) (*MONTHEND) (*NONE)) +
                          PROMPT('Schedule date')
             PARM       KWD(SCDDAY) TYPE(*CHAR) LEN(4) RSTD(*YES) MAX(7) +
                          DFT(*NONE) VALUES(*MON *TUE *WED *THU *FRI *SAT +
                          *SUN) SNGVAL((*NONE) (*ALL)) PROMPT('Schedule day')
             PARM       KWD(SCDTIME) TYPE(*TIME) DFT(*CURRENT) +
                          SPCVAL((*CURRENT)) PROMPT('Schedule time')
             PARM       KWD(RELDAYMON) TYPE(*INT2) MAX(5) DFT(1) +
                          RANGE(1 5) SPCVAL((*LAST 0)) PMTCTL(*PMTRQS) +
                          PROMPT('Relative day of month')
             PARM       KWD(SAVE) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*NO) +
                          VALUES(*NO *YES) PMTCTL(*PMTRQS) PROMPT('Save')
             PARM       KWD(OMITDATE) TYPE(*DATE) MAX(20) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Omit date')
             PARM       KWD(RCYACN) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SBMRLS) VALUES(*SBMRLS *SBMHLD *NOSBM) +
                          PMTCTL(*PMTRQS) PROMPT('Recovery action')
             PARM       KWD(JOBD) TYPE(Q1) DFT(*USRPRF) +
                          SNGVAL((*USRPRF)) PMTCTL(*PMTRQS) +
                          PROMPT('Job description')
             PARM       KWD(JOBQ) TYPE(Q1) DFT(*JOBD) SNGVAL((*JOBD)) +
                          PMTCTL(*PMTRQS) PROMPT('Job queue')
             PARM       KWD(USER) TYPE(*NAME) LEN(10) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*JOBD)) PMTCTL(*PMTRQS) +
                          PROMPT('User')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*USRPRF) +
                          SNGVAL((*USRPRF) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Message queue')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PMTCTL(*PMTRQS) +
                          PROMPT('Description')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
