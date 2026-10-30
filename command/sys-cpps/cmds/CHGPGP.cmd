/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGPGP: give an IFS object a new primary group. */
             CMD        PROMPT('Change Primary Group') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(CHGPGP)
             PARM       KWD(OBJ) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Object')
             PARM       KWD(NEWPGP) TYPE(*NAME) LEN(10) MIN(1) +
                          SPCVAL((*NONE)) EXPR(*YES) +
                          PROMPT('New primary group')
             PARM       KWD(DTAAUT) TYPE(*INT2) RSTD(*YES) DFT(*OLDPGP) +
                          SPCVAL((*OLDPGP 0) (*PRIVATE 1) (*RWX 2) +
                          (*RX 3) (*RW 4) (*WX 5) (*R 6) (*W 7) (*X 8) +
                          (*EXCLUDE 9) (*NONE 10)) EXPR(*YES) +
                          PROMPT('New data authority')
             PARM       KWD(OBJAUT) TYPE(*INT2) RSTD(*YES) DFT(*NONE) +
                          SPCVAL((*OBJEXIST 2) (*OBJMGT 3) (*OBJALTER 4) +
                          (*OBJREF 5)) SNGVAL((*NONE 0) (*ALL 1)) +
                          MAX(4) EXPR(*YES) PROMPT('New object authority')
             PARM       KWD(RVKOLDAUT) TYPE(*INT2) RSTD(*YES) DFT(*YES) +
                          SPCVAL((*NO 0) (*YES 1)) EXPR(*YES) +
                          PROMPT('Revoke old authority')
             PARM       KWD(SYMLNK) TYPE(*INT2) RSTD(*YES) DFT(*NO) +
                          SPCVAL((*NO 0) (*YES 1)) EXPR(*YES) +
                          PROMPT('Symbolic link')
             PARM       KWD(SUBTREE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL) +
                          PROMPT('Directory subtree')
