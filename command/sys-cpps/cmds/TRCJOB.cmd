/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* TRCJOB: the older job trace, switched on and off in the job. */
             CMD        PROMPT('Trace Job') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHSXCMD) HLPID(TRCJOB)
             PARM       KWD(SET) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*ON) +
                          VALUES(*ON *OFF *END) PROMPT('Trace option setting')
             PARM       KWD(TRCTYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *FLOW *DATA) +
                          PROMPT('Trace type')
             PARM       KWD(MAXSTG) TYPE(*INT4) DFT(16000) +
                          RANGE(1 16000) PROMPT('Maximum storage to use')
             PARM       KWD(TRCFULL) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*WRAP) VALUES(*WRAP *STOPTRC) +
                          PROMPT('Trace full')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*PRINT) VALUES(*PRINT) PROMPT('Output')
