/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RNMM: rename a member of a file. */
             CMD        PROMPT('Rename Member') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(RNMM)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(MBR) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Member')
             PARM       KWD(NEWMBR) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('New member')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
