/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKAUT: an IFS object's authorities, with options to change them. */
             CMD        PROMPT('Work with Authority') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(WRKAUT)
             PARM       KWD(OBJ) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Object')
             PARM       KWD(SYMLNK) TYPE(*INT2) RSTD(*YES) DFT(*NO) +
                          SPCVAL((*NO 0) (*YES 1)) EXPR(*YES) +
                          PROMPT('Symbolic link')
