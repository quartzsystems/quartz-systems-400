/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* INZPFM: initialize a physical file member with default or deleted */
/* records.                                                          */
             CMD        PROMPT('Initialize Physical File Mbr') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(INZPFM)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(MBR) TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST) (*LAST)) PROMPT('Member')
             PARM       KWD(RECORDS) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*DFT) VALUES(*DFT *DLT) +
                          PROMPT('Records to add')
             PARM       KWD(TOTRCDS) TYPE(*DEC) LEN(10) DFT(*NXTINCR) +
                          RANGE(1 2147483646) SPCVAL((*NXTINCR 0)) +
                          PROMPT('Total number of records')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
