/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGCLS: change a class. */
             CMD        PROMPT('Change Class') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CHGCLS)
             PARM       KWD(CLS) TYPE(Q1) MIN(1) PROMPT('Class')
             PARM       KWD(RUNPTY) TYPE(*INT2) DFT(*SAME) RANGE(1 99) +
                          SPCVAL((*SAME -1)) PROMPT('Run priority')
             PARM       KWD(TIMESLICE) TYPE(*INT4) DFT(*SAME) +
                          RANGE(8 9999999) SPCVAL((*SAME -1)) +
                          PROMPT('Time slice')
             PARM       KWD(PURGE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Eligible for purge')
             PARM       KWD(DFTWAIT) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 9999999) SPCVAL((*SAME -1) (*NOMAX 0)) +
                          PROMPT('Default wait time')
             PARM       KWD(CPUTIME) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 9999999) SPCVAL((*SAME -1) (*NOMAX 0)) +
                          PROMPT('Maximum CPU time')
             PARM       KWD(MAXTMPSTG) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 2147483647) +
                          SPCVAL((*SAME -1) (*NOMAX 0)) +
                          PROMPT('Maximum temporary storage')
             PARM       KWD(MAXTHD) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 32767) SPCVAL((*SAME -1) (*NOMAX 0)) +
                          PROMPT('Maximum threads')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) PROMPT('Description')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
