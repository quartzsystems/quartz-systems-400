/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGPFCST: enable or disable constraints of a physical file. */
             CMD        PROMPT('Change Physical File Constraint') +
                          MAXPOS(2) ALLOW(*ALL) HLPPNLGRP(QHDBCMD) +
                          HLPID(CHGPFCST)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(CST) TYPE(*CHAR) LEN(258) MIN(1) +
                          SPCVAL((*CHKPND) (*ALL)) MAX(300) +
                          PROMPT('Constraint')
             PARM       KWD(STATE) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *ENABLED *DISABLED) +
                          PROMPT('Constraint state')
             PARM       KWD(CHECK) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Verify constraint')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
