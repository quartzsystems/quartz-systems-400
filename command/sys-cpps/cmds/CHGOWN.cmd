/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGOWN: give an IFS object a new owner. */
             CMD        PROMPT('Change Owner') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(CHGOWN)
             PARM       KWD(OBJ) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Object')
             PARM       KWD(NEWOWN) TYPE(*NAME) LEN(10) MIN(1) +
                          EXPR(*YES) PROMPT('New owner')
             PARM       KWD(RVKOLDAUT) TYPE(*INT2) RSTD(*YES) DFT(*YES) +
                          SPCVAL((*NO 0) (*YES 1)) EXPR(*YES) +
                          PROMPT('Revoke current owner authority')
             PARM       KWD(SYMLNK) TYPE(*INT2) RSTD(*YES) DFT(*NO) +
                          SPCVAL((*NO 0) (*YES 1)) EXPR(*YES) +
                          PROMPT('Symbolic link')
             PARM       KWD(SUBTREE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL) +
                          PROMPT('Directory subtree')
