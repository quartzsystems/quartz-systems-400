/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPCPCST: display the records that keep a constraint in check pending. */
             CMD        PROMPT('Display Check Pending Constraint') +
                          MAXPOS(2) ALLOW(*ALL) HLPPNLGRP(QHDBCMD) +
                          HLPID(DSPCPCST)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(CST) TYPE(*CHAR) LEN(258) MIN(1) +
                          PROMPT('Constraint')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
