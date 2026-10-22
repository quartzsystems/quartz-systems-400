/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTSBSD: create a subsystem description. */
             CMD        PROMPT('Create Subsystem Description') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CRTSBSD)
             PARM       KWD(SBSD) TYPE(Q1) MIN(1) +
                          PROMPT('Subsystem description')
             PARM       KWD(POOLS) TYPE(E1) MAX(10) DFT((1 *BASE)) +
                          PROMPT('Storage pools')
             PARM       KWD(MAXJOBS) TYPE(*INT4) DFT(*NOMAX) +
                          RANGE(0 1000) SPCVAL((*NOMAX -1)) +
                          PROMPT('Maximum jobs')
             PARM       KWD(SGNDSPF) TYPE(Q2) DFT(QDSIGNON) +
                          PMTCTL(*PMTRQS) PROMPT('Sign-on display file')
             PARM       KWD(SYSLIBLE) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Subsystem library')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) PROMPT('Authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT2) RANGE(1 10) +
                          PROMPT('Pool identifier')
                        ELEM       TYPE(*CHAR) LEN(10) +
                          PROMPT('Storage size or name')
                        ELEM       TYPE(*INT4) DFT(1) RANGE(0 32767) +
                          PROMPT('Activity level')
