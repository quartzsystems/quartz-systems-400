/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDTRC: end a trace session, keeping or deleting its data. */
             CMD        PROMPT('End Trace') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHSXCMD) HLPID(ENDTRC)
             PARM       KWD(SSNID) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Session ID')
             PARM       KWD(DTAOPT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*LIB) VALUES(*LIB *DLT) PROMPT('Data option')
             PARM       KWD(DTALIB) TYPE(*NAME) LEN(10) DFT(QGPL) +
                          PROMPT('Data library')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PROMPT('Replace data')
             PARM       KWD(PRTTRC) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PROMPT('Print trace data')
