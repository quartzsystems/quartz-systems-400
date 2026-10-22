/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKSYSACT: work with the system activity, jobs and tasks by */
/* processor use.                                                 */
             CMD        PROMPT('Work with System Activity') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(WRKSYSACT)
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(5) RSTD(*YES) DFT(*) +
                          VALUES(* *FILE *BOTH) PROMPT('Output')
             PARM       KWD(INTERVAL) TYPE(*INT4) DFT(5) RANGE(1 999) +
                          PROMPT('Interval, seconds')
             PARM       KWD(INFTYPE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*JOBTASK) VALUES(*JOBTASK *JOB *TASK) +
                          PROMPT('Information type')
             PARM       KWD(SEQ) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*CPU) +
                          VALUES(*CPU *IO) PROMPT('Sequence')
             PARM       KWD(LIB) TYPE(*NAME) LEN(10) DFT(QPFRDATA) +
                          PROMPT('Library')
             PARM       KWD(MBR) TYPE(*NAME) LEN(10) DFT(QAITMON) +
                          PROMPT('Member')
