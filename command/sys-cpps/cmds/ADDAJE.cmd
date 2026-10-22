/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDAJE: add an autostart job entry to a subsystem description. */
             CMD        PROMPT('Add Autostart Job Entry') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(ADDAJE)
             PARM       KWD(SBSD) TYPE(Q1) MIN(1) +
                          PROMPT('Subsystem description')
             PARM       KWD(JOB) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Job name')
             PARM       KWD(JOBD) TYPE(Q2) DFT(*USRPRF) SNGVAL((*USRPRF)) +
                          PROMPT('Job description')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
