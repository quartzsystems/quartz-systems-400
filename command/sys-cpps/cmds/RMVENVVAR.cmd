/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVENVVAR: remove an environment variable of the job or the system. */
             CMD        PROMPT('Remove Environment Variable') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHP0ZCMD) +
                          HLPID(RMVENVVAR)
             PARM       KWD(ENVVAR) TYPE(*CHAR) LEN(128) MIN(1) +
                          SPCVAL((*ALL)) CASE(*MIXED) +
                          PROMPT('Environment variable')
             PARM       KWD(LEVEL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*JOB) VALUES(*JOB *SYS) PMTCTL(*PMTRQS) +
                          PROMPT('Level')
