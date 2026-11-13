/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPMFSINF: the attributes of the file system an object is in (STATFS  */
/* is the same command).                                              */
             CMD        PROMPT('Display Mounted FS Information') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) +
                          HLPID(DSPMFSINF)
             PARM       KWD(OBJ) TYPE(*PNAME) LEN(5000) MIN(1) +
                          CHOICE('Path name') PROMPT('Object')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*) SPCVAL((* *) (*PRINT P)) +
                          PROMPT('Output')
