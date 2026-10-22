/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPSYSVAL: display a system value. */
             CMD        PROMPT('Display System Value') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) HLPID(DSPSYSVAL)
             PARM       KWD(SYSVAL) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('System value')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
