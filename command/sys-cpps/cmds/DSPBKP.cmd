/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPBKP: show the breakpoints set in the programs under debug. */
             CMD        PROMPT('Display Breakpoints') ALLOW(*ALL) +
                          HLPPNLGRP(QHDBGCMD) HLPID(DSPBKP)
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
