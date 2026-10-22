/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVPJE: remove a prestart job entry from a subsystem description. */
             CMD        PROMPT('Remove Prestart Job Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(RMVPJE)
             PARM       KWD(SBSD) TYPE(Q1) MIN(1) +
                          PROMPT('Subsystem description')
             PARM       KWD(PGM) TYPE(Q1) MIN(1) PROMPT('Program')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
