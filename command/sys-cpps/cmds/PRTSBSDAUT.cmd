/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* PRTSBSDAUT: print the subsystem descriptions whose entries name a */
/* default user.                                                       */
             CMD        PROMPT('Print Subsystem Desc Auth') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(PRTSBSDAUT)
             PARM       KWD(LIB) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*ALLUSR) (*LIBL) (*CURLIB)) +
                          PROMPT('Library')
