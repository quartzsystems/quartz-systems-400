/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* PRTQAUT: print the authorities to the job and output queues of a */
/* library.                                                           */
             CMD        PROMPT('Print Queue Authority') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(PRTQAUT)
             PARM       KWD(LIB) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*ALLUSR) (*LIBL) (*CURLIB)) +
                          PROMPT('Library')
