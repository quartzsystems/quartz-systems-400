/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKRDBDIRE: list the relational database directory. */
             CMD        PROMPT('Work with RDB Directory Entries') +
                          MAXPOS(2) ALLOW(*ALL) HLPPNLGRP(QHRWCMD) +
                          HLPID(WRKRDBDIRE)
             PARM       KWD(RDB) TYPE(*CHAR) LEN(18) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Entry')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(1) RSTD(*YES) DFT(*) +
                          SPCVAL((* *) (*PRINT P)) PROMPT('Output')
