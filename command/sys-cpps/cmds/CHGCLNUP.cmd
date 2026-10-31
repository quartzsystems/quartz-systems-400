/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGCLNUP: change the cleanup options. */
             CMD        PROMPT('Change Cleanup') MAXPOS(0) ALLOW(*ALL) +
                          PMTOVRPGM(QSYS/QEZCLNPO) HLPPNLGRP(QHWMCMD) +
                          HLPID(CHGCLNUP)
             PARM       KWD(ALWCLNUP) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Allow cleanup')
             PARM       KWD(STRTIME) TYPE(*CHAR) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*SCDPWROFF)) +
                          PROMPT('Time cleanup starts each day')
             PARM       KWD(USRMSG) TYPE(E1) +
                          PROMPT('Number of days to keep')
             PARM       KWD(SYSMSG) TYPE(*CHAR) LEN(5) DFT(*SAME) +
                          SPCVAL((*SAME) (*KEEP)) +
                          PROMPT('System and workstation msgs')
             PARM       KWD(CRITSYSMSG) TYPE(*CHAR) LEN(5) DFT(*SAME) +
                          SPCVAL((*SAME) (*KEEP)) +
                          PROMPT('Critical system messages')
             PARM       KWD(SYSPRT) TYPE(*CHAR) LEN(5) DFT(*SAME) +
                          SPCVAL((*SAME) (*KEEP)) +
                          PROMPT('Job logs and system output')
             PARM       KWD(SYSLOG) TYPE(*CHAR) LEN(5) DFT(*SAME) +
                          SPCVAL((*SAME) (*KEEP)) +
                          PROMPT('System journals and logs')
             PARM       KWD(JOBQ) TYPE(Q1) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Job queue')
             PARM       KWD(JRNRCVSIZ) TYPE(*CHAR) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME)) PMTCTL(*PMTRQS) +
                          PROMPT('Journal receiver size')
             PARM       KWD(CALITM) TYPE(*CHAR) LEN(5) DFT(*SAME) +
                          SPCVAL((*SAME) (*KEEP)) PMTCTL(*PMTRQS) +
                          PROMPT('Office calendar items')
             PARM       KWD(RUNPTY) TYPE(*CHAR) LEN(5) DFT(*SAME) +
                          SPCVAL((*SAME)) PMTCTL(*PMTRQS) +
                          PROMPT('Run priority')
 E1:         ELEM       TYPE(*CHAR) LEN(5) DFT(*SAME) +
                          SPCVAL((*SAME) (*KEEP)) PROMPT('User messages')
 Q1:         QUAL       TYPE(*NAME) LEN(10)
             QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
