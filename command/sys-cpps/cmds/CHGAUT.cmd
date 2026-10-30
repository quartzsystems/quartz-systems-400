/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGAUT: change users' authority to an IFS object. */
             CMD        PROMPT('Change Authority') MAXPOS(4) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(CHGAUT)
             PARM       KWD(OBJ) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Object')
             PARM       KWD(USER) TYPE(*NAME) LEN(10) SNGVAL((*PUBLIC)) +
                          MAX(50) EXPR(*YES) PROMPT('User')
             PARM       KWD(DTAAUT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NONE *RWX *RX *RW +
                          *WX *R *W *X *EXCLUDE *AUTL) EXPR(*YES) +
                          PROMPT('New data authorities')
             PARM       KWD(OBJAUT) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*OBJEXIST *OBJMGT +
                          *OBJALTER *OBJREF) SNGVAL((*SAME) (*NONE) +
                          (*ALL)) MAX(4) EXPR(*YES) +
                          PROMPT('New object authorities')
             PARM       KWD(AUTL) TYPE(*NAME) LEN(10) SPCVAL((*NONE)) +
                          EXPR(*YES) PROMPT('Authorization list')
             PARM       KWD(SYMLNK) TYPE(*INT2) RSTD(*YES) DFT(*NO) +
                          SPCVAL((*NO 0) (*YES 1)) EXPR(*YES) +
                          PROMPT('Symbolic link')
             PARM       KWD(SUBTREE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL) +
                          PROMPT('Directory subtree')
