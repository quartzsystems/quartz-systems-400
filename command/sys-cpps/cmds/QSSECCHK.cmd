/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSSECCHK: check the running system against the security model and */
/* print pass, warning or failure for each check.                       */
             CMD        PROMPT('Check System Security') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(QSSECCHK)
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*PRINT) VALUES(* *PRINT) PROMPT('Output')
