/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVPFCST: remove constraints from a physical file. */
             CMD        PROMPT('Remove Physical File Constraint') +
                          MAXPOS(2) ALLOW(*ALL) HLPPNLGRP(QHDBCMD) +
                          HLPID(RMVPFCST)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(CST) TYPE(*CHAR) LEN(258) MIN(1) +
                          SPCVAL((*CHKPND) (*ALL)) PROMPT('Constraint')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *REFCST *UNQCST *PRIKEY +
                          *CHKCST) PROMPT('Constraint type')
             PARM       KWD(RMVCST) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*RESTRICT) VALUES(*RESTRICT *REMOVE +
                          *KEEP) PROMPT('Remove constraint')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
