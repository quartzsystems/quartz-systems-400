/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPF: browse a stream file or a database member, read only. */
             CMD        PROMPT('Display File') MAXPOS(3) ALLOW(*ALL) +
                          HLPPNLGRP(QHZRUCMD) +
                          HLPID(DSPF)
             PARM       KWD(STMF) TYPE(*PNAME) LEN(300) EXPR(*YES) +
                          PROMPT('Stream file, or')
             PARM       KWD(FILE) TYPE(Q1) EXPR(*YES) +
                          PROMPT('Data base file')
             PARM       KWD(MBR) TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) EXPR(*YES) PMTCTL(P1) +
                          PROMPT('Member')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1) EXPR(*YES)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) EXPR(*YES) +
                          PROMPT('Library')
             P1:        PMTCTL     CTL(FILE) COND((*SPCFD))
