/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTCLS: create a class. */
             CMD        PROMPT('Create Class') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CRTCLS)
             PARM       KWD(CLS) TYPE(Q1) MIN(1) PROMPT('Class')
             PARM       KWD(RUNPTY) TYPE(*INT2) DFT(50) RANGE(1 99) +
                          PROMPT('Run priority')
             PARM       KWD(TIMESLICE) TYPE(*INT4) DFT(2000) +
                          RANGE(0 9999999) PROMPT('Time slice')
             PARM       KWD(PURGE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Eligible for purge')
             PARM       KWD(DFTWAIT) TYPE(*INT4) DFT(30) +
                          RANGE(1 9999999) SPCVAL((*NOMAX 0)) +
                          PROMPT('Default wait time')
             PARM       KWD(CPUTIME) TYPE(*INT4) DFT(*NOMAX) +
                          RANGE(1 9999999) SPCVAL((*NOMAX 0)) +
                          PROMPT('Maximum CPU time')
             PARM       KWD(MAXTMPSTG) TYPE(*INT4) DFT(*NOMAX) +
                          RANGE(1 2147483647) SPCVAL((*NOMAX 0)) +
                          PROMPT('Maximum temporary storage')
             PARM       KWD(MAXTHD) TYPE(*INT4) DFT(*NOMAX) +
                          RANGE(1 32767) SPCVAL((*NOMAX 0)) +
                          PROMPT('Maximum threads')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
