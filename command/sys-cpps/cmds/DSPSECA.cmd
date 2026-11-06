/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPSECA: the security attributes: next ID numbers, levels, DST flags. */
             CMD        PROMPT('Display Security Attributes') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHSYCMD) +
                          HLPID(DSPSECA)
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) EXPR(*YES) +
                          PROMPT('Output')
