/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRSDA: the Screen Design Aid, at its menu or one of its options. */
             CMD        PROMPT('Start SDA') MAXPOS(3) +
                          ALLOW(*INTERACT *IPGM *IREXX *EXEC *IMOD) +
                          HLPPNLGRP(QHSDACMD) HLPID(STRSDA)
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*SELECT) VALUES(1 2 3) +
                          SPCVAL((*SELECT ' ')) PROMPT('Option')
             PARM       KWD(SRCFILE) TYPE(Q1) DFT(QDDSSRC) +
                          PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*SELECT) +
                          SPCVAL((*SELECT)) PROMPT('Source member')
             PARM       KWD(TSTFILE) TYPE(Q2) DFT(*SELECT) +
                          SNGVAL((*SELECT)) PROMPT('Test display file')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
