/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPPRB: show logged problems in detail. */
             CMD        PROMPT('Display Problem') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHSXCMD) HLPID(DSPPRB)
             PARM       KWD(PRBID) TYPE(*CHAR) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Problem identifier')
             PARM       KWD(STATUS) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ALL) MAX(7) SNGVAL((*ALL)) VALUES(*OPENED +
                          *READY *PREPARED *SENT *ANSWERED *VERIFIED +
                          *CLOSED) PROMPT('Status')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
