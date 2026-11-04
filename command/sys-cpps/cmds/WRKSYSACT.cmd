/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKSYSACT: work with the system activity, jobs and tasks by */
/* processor use (QS00PT1, Performance Tools).                    */
             CMD        PROMPT('Work with System Activity') MAXPOS(0) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(WRKSYSACT)
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(5) RSTD(*YES) DFT(*) +
                          VALUES(* *FILE *BOTH) PROMPT('Output')
             PARM       KWD(INTERVAL) TYPE(*INT4) DFT(5) RANGE(1 900) +
                          CHOICE('1 - 900 seconds') +
                          PROMPT('Interval length')
             PARM       KWD(SEQ) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*CPU) VALUES(*CPU *DBCPU *IO *STGALC +
                          *STGDLC) CHOICE('*CPU, *DBCPU, *IO, *STGALC...') +
                          PROMPT('Sequence')
             PARM       KWD(INFTYPE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *JOBS *TASKS) +
                          PROMPT('Type of information')
             PARM       KWD(NBROFITV) TYPE(*INT4) DFT(1) RANGE(1 1024) +
                          PMTCTL(P1) PROMPT('Number of intervals')
             PARM       KWD(LIB) TYPE(*NAME) LEN(10) DFT(QPFRDATA) +
                          PMTCTL(P1) PROMPT('Library')
             PARM       KWD(MBR) TYPE(*NAME) LEN(10) DFT(QAITMON) +
                          PMTCTL(P1) PROMPT('Member')
 P1:         PMTCTL     CTL(OUTPUT) COND((*NE '*'))
