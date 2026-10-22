/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVWSE: remove a workstation entry from a subsystem description. */
             CMD        PROMPT('Remove Work Station Entry') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(RMVWSE)
             PARM       KWD(SBSD) TYPE(Q1) MIN(1) +
                          PROMPT('Subsystem description')
             PARM       KWD(WRKSTN) TYPE(*GENERIC) LEN(10) +
                          PROMPT('Work station name')
             PARM       KWD(WRKSTNTYPE) TYPE(*CHAR) LEN(10) +
                          SPCVAL((*ALL) (*NONASCII) (*ASCII)) +
                          PROMPT('Work station type')
             DEP        CTL(*ALWAYS) PARM(&WRKSTN &WRKSTNTYPE) +
                          NBRTRUE((*EQ 1))
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
