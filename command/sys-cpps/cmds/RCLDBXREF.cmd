/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RCLDBXREF: check, or put right, the database cross-reference. */
             CMD        PROMPT('Reclaim DB Cross-Reference') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSXCMD) HLPID(RCLDBXREF)
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(6) RSTD(*YES) MIN(1) +
                          VALUES(*CHECK *FIX) PROMPT('Option')
             PARM       KWD(LIB) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*ERR)) PROMPT('Library')
