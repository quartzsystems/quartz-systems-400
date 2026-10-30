/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPUDFS: a user-defined file system's attributes and mounts. */
             CMD        PROMPT('Display User-Defined FS') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(DSPUDFS)
             PARM       KWD(UDFS) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('User-defined file system')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*) SPCVAL((* *) (*PRINT P)) EXPR(*YES) +
                          PROMPT('Output')
