/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVAJE: remove an autostart job entry from a subsystem description. */
             CMD        PROMPT('Remove Autostart Job Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(RMVAJE)
             PARM       KWD(SBSD) TYPE(Q1) MIN(1) +
                          PROMPT('Subsystem description')
             PARM       KWD(JOB) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Job name')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
