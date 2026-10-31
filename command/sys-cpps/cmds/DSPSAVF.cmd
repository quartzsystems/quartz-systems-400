/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPSAVF: show what a save file holds (save files come later). */
             CMD        PROMPT('Display Save File') MAXPOS(1) +
                          ALLOW(*INTERACT *IPGM *IREXX *EXEC *IMOD) +
                          HLPPNLGRP(QHSRCMD) HLPID(DSPSAVF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('Save file')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
