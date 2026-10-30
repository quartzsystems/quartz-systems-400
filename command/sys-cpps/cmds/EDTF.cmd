/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* EDTF: edit a stream file or a database member, CCSID-aware. */
             CMD        PROMPT('Edit File') MAXPOS(2) ALLOW(*INTERACT *IPGM +
                          *IMOD *IREXX *EXEC) HLPPNLGRP(QHZRUCMD) +
                          HLPID(EDTF)
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
