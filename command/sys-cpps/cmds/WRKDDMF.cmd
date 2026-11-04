/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKDDMF: list DDM files and where each goes. */
             CMD        PROMPT('Work with DDM Files') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDDCMD) HLPID(WRKDDMF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(1) RSTD(*YES) DFT(*) +
                          SPCVAL((* *) (*PRINT P)) PROMPT('Output')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) SPCVAL((*ALL)) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALL) +
                          (*ALLUSR)) PROMPT('Library')
