/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKENVVAR: work with the job's or the system's environment variables. */
             CMD        PROMPT('Work with Environment Var') +
                          ALLOW(*ALL) HLPPNLGRP(QHP0ZCMD) HLPID(WRKENVVAR)
             PARM       KWD(LEVEL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*JOB) VALUES(*JOB *SYS) PMTCTL(*PMTRQS) +
                          PROMPT('Level')
