/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGJOBSCDE: change an entry of the job schedule. */
             CMD        PROMPT('Change Job Schedule Entry') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CHGJOBSCDE)
             PARM       KWD(JOB) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Job name')
             PARM       KWD(ENTRYNBR) TYPE(*INT4) DFT(*ONLY) +
                          RANGE(1 999999) SPCVAL((*ONLY 0)) +
                          PROMPT('Entry number')
             PARM       KWD(CMD) TYPE(*CMDSTR) LEN(512) DFT(*SAME) +
                          PROMPT('Command to run')
             PARM       KWD(FRQ) TYPE(*CHAR) LEN(8) RSTD(*YES) DFT(*SAME) +
                          VALUES(*SAME *ONCE *WEEKLY *MONTHLY) +
                          PROMPT('Frequency')
             PARM       KWD(SCDDATE) TYPE(*DATE) DFT(*SAME) +
                          SPCVAL((*SAME) (*CURRENT) (*MONTHSTR) (*MONTHEND) +
                          (*NONE)) PROMPT('Schedule date')
             PARM       KWD(SCDDAY) TYPE(*CHAR) LEN(4) RSTD(*YES) MAX(7) +
                          DFT(*SAME) VALUES(*MON *TUE *WED *THU *FRI *SAT +
                          *SUN) SNGVAL((*SAME) (*NONE) (*ALL)) +
                          PROMPT('Schedule day')
             PARM       KWD(SCDTIME) TYPE(*TIME) DFT(*SAME) +
                          SPCVAL((*SAME) (*CURRENT)) PROMPT('Schedule time')
             PARM       KWD(RELDAYMON) TYPE(*INT2) MAX(5) DFT(*SAME) +
                          RANGE(1 5) SPCVAL((*LAST 0)) SNGVAL((*SAME)) +
                          PROMPT('Relative day of month')
             PARM       KWD(SAVE) TYPE(*CHAR) LEN(5) RSTD(*YES) DFT(*SAME) +
                          VALUES(*SAME *NO *YES) PROMPT('Save')
             PARM       KWD(OMITDATE) TYPE(*DATE) MAX(20) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) PROMPT('Omit date')
             PARM       KWD(RCYACN) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SBMRLS *SBMHLD *NOSBM) +
                          PROMPT('Recovery action')
             PARM       KWD(JOBD) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*USRPRF)) PROMPT('Job description')
             PARM       KWD(JOBQ) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*JOBD)) PROMPT('Job queue')
             PARM       KWD(USER) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*CURRENT) (*JOBD)) PROMPT('User')
             PARM       KWD(MSGQ) TYPE(Q1) DFT(*SAME) +
                          SNGVAL((*SAME) (*USRPRF) (*NONE)) +
                          PROMPT('Message queue')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) PROMPT('Description')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
