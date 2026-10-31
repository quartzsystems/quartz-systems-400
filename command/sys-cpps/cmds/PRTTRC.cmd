/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* PRTTRC: print trace data ENDTRC kept. */
             CMD        PROMPT('Print Trace Data') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHSXCMD) HLPID(PRTTRC)
             PARM       KWD(DTAMBR) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Data member')
             PARM       KWD(DTALIB) TYPE(*NAME) LEN(10) DFT(QGPL) +
                          PROMPT('Data library')
             PARM       KWD(DLTTRC) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PROMPT('Delete trace')
