/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* TFRGRPJOB: move the display to another job of the group, */
/* starting it when the group has none of that name.           */
             CMD        PROMPT('Transfer to Group Job') +
                          ALLOW(*INTERACT *IPGM *IMOD *IREXX *EXEC) +
                          HLPPNLGRP(QHWCCMD) HLPID(TFRGRPJOB)
             PARM       KWD(GRPJOB) TYPE(*NAME) LEN(10) DFT(*PRV) +
                          SPCVAL((*PRV) (*SELECT)) PROMPT('Group job')
             PARM       KWD(INLGRPPGM) TYPE(Q1) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Initial group program')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
