/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKSBSJOB: work with the jobs of a subsystem, or those waiting on */
/* job queues or output queues.                                       */
             CMD        PROMPT('Work with Subsystem Jobs') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(WRKSBSJOB)
             PARM       KWD(SBS) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*OUTQ) (*JOBQ)) PROMPT('Subsystem')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
