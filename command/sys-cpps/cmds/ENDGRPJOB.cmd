/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDGRPJOB: end a job of the group, or this one. */
             CMD        PROMPT('End Group Job') +
                          ALLOW(*INTERACT *IPGM *IMOD *IREXX *EXEC) +
                          HLPPNLGRP(QHWCCMD) HLPID(ENDGRPJOB)
             PARM       KWD(GRPJOB) TYPE(*NAME) LEN(10) DFT(*) +
                          SPCVAL((*)) PROMPT('Group job')
             PARM       KWD(RSPGRPJOB) TYPE(*NAME) LEN(10) DFT(*PRV) +
                          SPCVAL((*PRV)) PROMPT('Group job to be resumed')
             PARM       KWD(LOG) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NOLIST) VALUES(*NOLIST *LIST) +
                          PROMPT('Job log')
