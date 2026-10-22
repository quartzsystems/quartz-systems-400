/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CALL: call a program with parameters. */
             CMD        PROMPT('Call Program') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHCLCMD) HLPID(CALL)
             PARM       KWD(PGM) TYPE(Q1) MIN(1) PGM(*YES) +
                          PROMPT('Program')
             PARM       KWD(PARM) TYPE(*X) LEN(5000) MAX(255) +
                          EXPR(*YES) PROMPT('Parameters')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
