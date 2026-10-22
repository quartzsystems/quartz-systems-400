/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CPYSRCF: copy source members between source files. */
             CMD        PROMPT('Copy Source File') MAXPOS(4) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CPYSRCF)
             PARM       KWD(FROMFILE) TYPE(Q1) MIN(1) PROMPT('From file')
             PARM       KWD(TOFILE) TYPE(Q1) MIN(1) PROMPT('To file')
             PARM       KWD(FROMMBR) TYPE(*GENERIC) LEN(10) MIN(1) +
                          SPCVAL((*ALL)) PROMPT('From member')
             PARM       KWD(TOMBR) TYPE(*NAME) LEN(10) DFT(*FROMMBR) +
                          SPCVAL((*FROMMBR)) PROMPT('To member')
             PARM       KWD(MBROPT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
             PARM       KWD(SRCCHGDATE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NEW) VALUES(*NEW *OLD) +
                          PROMPT('Source change date')
             PARM       KWD(SRCOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SEQNBR *DATE) MAX(2) +
                          PROMPT('Source update options')
             PARM       KWD(SRCSEQ) TYPE(E1) +
                          PROMPT('Source sequence numbering')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*DEC) LEN(6 2) DFT(1.00) +
                          RANGE(0.01 9999.99) PROMPT('Starting number')
                        ELEM       TYPE(*DEC) LEN(6 2) DFT(1.00) +
                          RANGE(0.01 9999.99) PROMPT('Increment')
