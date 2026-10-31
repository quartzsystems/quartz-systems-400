/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRTRC: start a job trace session. */
             CMD        PROMPT('Start Trace') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHSXCMD) HLPID(STRTRC)
             PARM       KWD(SSNID) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Session ID')
             PARM       KWD(JOB) TYPE(QJ) DFT(*) SNGVAL((*ALL)) +
                          PROMPT('Jobs')
             PARM       KWD(MAXSTG) TYPE(*INT4) DFT(10000) +
                          RANGE(1 4000000) PROMPT('Maximum storage to use')
             PARM       KWD(TRCFULL) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*WRAP) VALUES(*WRAP *STOPTRC) +
                          PROMPT('Trace full')
             PARM       KWD(JOBTRCTYPE) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *FLOW *DATA) +
                          PROMPT('Job trace type')
             QJ:        QUAL       TYPE(*NAME) LEN(10) SPCVAL((*))
                        QUAL       TYPE(*NAME) LEN(10) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) RANGE(000000 999999) +
                          FULL(*YES) PROMPT('Number')
