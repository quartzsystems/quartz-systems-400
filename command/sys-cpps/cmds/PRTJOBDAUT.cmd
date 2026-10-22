/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* PRTJOBDAUT: print the job descriptions that name a user and are not */
/* excluded from the public.                                             */
             CMD        PROMPT('Print Job Description Auth') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(PRTJOBDAUT)
             PARM       KWD(LIB) TYPE(*NAME) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*ALLUSR) (*LIBL) (*CURLIB)) +
                          PROMPT('Library')
