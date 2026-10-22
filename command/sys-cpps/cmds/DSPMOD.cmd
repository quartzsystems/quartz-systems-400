/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPMOD: show the attributes, exports, imports and procedures of a */
/* module.                                                             */
             CMD        PROMPT('Display Module') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(DSPMOD)
             PARM       KWD(MODULE) TYPE(Q1) MIN(1) PROMPT('Module')
             PARM       KWD(DETAIL) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ALL) MAX(7) VALUES(*ALL *BASIC *SIZE +
                          *EXPORT *IMPORT *PROCLIST *REFSYSOBJ +
                          *COPYRIGHT) SNGVAL((*ALL)) PROMPT('Detail')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
