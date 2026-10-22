/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPCMD: show the attributes of a command. */
             CMD        PROMPT('Display Command') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHPGCMD) HLPID(DSPCMD)
             PARM       KWD(CMD) TYPE(Q1) MIN(1) PROMPT('Command')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) DFT(*) +
                          VALUES(* *PRINT) PROMPT('Output')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
