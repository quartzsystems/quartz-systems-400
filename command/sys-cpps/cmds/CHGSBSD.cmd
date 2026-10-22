/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGSBSD: change a subsystem description. */
             CMD        PROMPT('Change Subsystem Description') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CHGSBSD)
             PARM       KWD(SBSD) TYPE(Q1) MIN(1) +
                          PROMPT('Subsystem description')
             PARM       KWD(POOLS) TYPE(E1) MAX(10) DFT(*SAME) +
                          SNGVAL((*SAME)) PROMPT('Storage pools')
             PARM       KWD(MAXJOBS) TYPE(*INT4) DFT(*SAME) +
                          RANGE(0 1000) SPCVAL((*SAME -2) (*NOMAX -1)) +
                          PROMPT('Maximum jobs')
             PARM       KWD(SGNDSPF) TYPE(Q2) DFT(*SAME) SNGVAL((*SAME)) +
                          PROMPT('Sign-on display file')
             PARM       KWD(SYSLIBLE) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) PROMPT('Subsystem library')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) PROMPT('Description')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*INT2) RANGE(1 10) +
                          PROMPT('Pool identifier')
                        ELEM       TYPE(*CHAR) LEN(10) +
                          PROMPT('Storage size or name')
                        ELEM       TYPE(*INT4) DFT(1) RANGE(0 32767) +
                          PROMPT('Activity level')
