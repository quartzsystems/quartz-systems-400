/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPAUT: an IFS object's authorities. */
             CMD        PROMPT('Display Authority') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(DSPAUT)
             PARM       KWD(OBJ) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Object')
             PARM       KWD(SYMLNK) TYPE(*INT2) RSTD(*YES) DFT(*NO) +
                          SPCVAL((*NO 0) (*YES 1)) EXPR(*YES) +
                          PROMPT('Symbolic link')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(1) RSTD(*YES) +
                          DFT(*) SPCVAL((* *) (*PRINT P)) EXPR(*YES) +
                          PROMPT('Output')
