/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* TFRJOB: transfer the interactive job to another job queue. */
             CMD        PROMPT('Transfer Job') MAXPOS(2) +
                          ALLOW(*INTERACT *IPGM *IMOD *EXEC) +
                          HLPPNLGRP(QHWMCMD) HLPID(TFRJOB)
             PARM       KWD(JOBQ) TYPE(Q1) MIN(1) PROMPT('Job queue')
             PARM       KWD(RTGDTA) TYPE(*CHAR) LEN(80) DFT(*JOBD) +
                          SPCVAL((*JOBD) (*RQSDTA)) PROMPT('Routing data')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
