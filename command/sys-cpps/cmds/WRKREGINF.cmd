/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKREGINF: list the exit points and work with their programs. */
             CMD        PROMPT('Work with Registration Info') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHUSCMD) HLPID(WRKREGINF)
             PARM       KWD(EXITPNT) TYPE(*GENERIC) LEN(20) +
                          DFT(*REGISTERED) SPCVAL((*REGISTERED) +
                          (*UNREGISTERED) (*ALL)) PROMPT('Exit point')
             PARM       KWD(FORMAT) TYPE(*GENERIC) LEN(8) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Exit point format')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*) SPCVAL((* D) (*PRINT P)) EXPR(*YES) +
                          PROMPT('Output')
