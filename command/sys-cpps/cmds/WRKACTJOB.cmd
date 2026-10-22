/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKACTJOB: work with the active jobs and their statistics. */
             CMD        PROMPT('Work with Active Jobs') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(WRKACTJOB)
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(RESET) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*NO) +
                          VALUES(*NO *YES) PROMPT('Reset statistics')
             PARM       KWD(SBS) TYPE(*NAME) LEN(10) MAX(10) DFT(*ALL) +
                          SNGVAL((*ALL)) PROMPT('Subsystem')
             PARM       KWD(CPUPCTLMT) TYPE(*DEC) LEN(4 1) DFT(*NONE) +
                          RANGE(0 99.9) SPCVAL((*NONE -1)) +
                          PROMPT('CPU percent limit')
             PARM       KWD(RSPLMT) TYPE(*DEC) LEN(4 1) DFT(*NONE) +
                          RANGE(0 999.9) SPCVAL((*NONE -1)) +
                          PROMPT('Response time limit')
             PARM       KWD(SEQ) TYPE(*CHAR) LEN(9) RSTD(*YES) DFT(*SBS) +
                          VALUES(*SBS *CPUPCT *FUNCTION *IOTOTAL *JOB +
                          *POOL *PTY *RSP *STS *TYPE *USER) +
                          PROMPT('Sequence')
             PARM       KWD(JOB) TYPE(QJ) DFT(*ALL) PROMPT('Job name')
             QJ:        QUAL       TYPE(*GENERIC) LEN(10) SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('User')
                        QUAL       TYPE(*CHAR) LEN(6) DFT(*ALL) +
                          RANGE(000000 999999) SPCVAL((*ALL)) FULL(*YES) +
                          PROMPT('Number')
