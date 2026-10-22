/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGSRCPF: change a source physical file's attributes. */
             CMD        PROMPT('Change Source Physical File') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CHGSRCPF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(SYSTEM) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*LCL) VALUES(*LCL *RMT *FILETYPE) +
                          PROMPT('System')
             PARM       KWD(EXPDATE) TYPE(*DATE) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) +
                          PROMPT('Expiration date for member')
             PARM       KWD(MAXMBRS) TYPE(*DEC) LEN(5) DFT(*SAME) +
                          RANGE(1 32767) SPCVAL((*SAME -1) (*NOMAX 0)) +
                          PROMPT('Maximum members')
             PARM       KWD(ACCPTHSIZ) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *MAX1TB *MAX4GB) +
                          PROMPT('Access path size')
             PARM       KWD(SIZE) TYPE(E1) DFT(*SAME) SNGVAL((*SAME) +
                          (*NOMAX)) PROMPT('Member size')
             PARM       KWD(ALLOCATE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Allocate storage')
             PARM       KWD(UNIT) TYPE(*DEC) LEN(3) DFT(*SAME) +
                          RANGE(1 255) SPCVAL((*SAME -1) (*ANY 0)) +
                          PROMPT('Preferred storage unit')
             PARM       KWD(FRCRATIO) TYPE(*DEC) LEN(5) DFT(*SAME) +
                          RANGE(1 32767) SPCVAL((*SAME -1) (*NONE 0)) +
                          PROMPT('Records to force a write')
             PARM       KWD(WAITFILE) TYPE(*DEC) LEN(5) DFT(*SAME) +
                          RANGE(1 32767) SPCVAL((*SAME -2) (*IMMED 0) +
                          (*CLS -1)) PROMPT('Maximum file wait time')
             PARM       KWD(WAITRCD) TYPE(*DEC) LEN(5) DFT(*SAME) +
                          RANGE(1 32767) SPCVAL((*SAME -2) (*IMMED 0) +
                          (*NOMAX -1)) PROMPT('Maximum record wait time')
             PARM       KWD(SHARE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Share open data path')
             PARM       KWD(DLTPCT) TYPE(*DEC) LEN(3) DFT(*SAME) +
                          RANGE(1 100) SPCVAL((*SAME -1) (*NONE 0)) +
                          PROMPT('Max % deleted records allowed')
             PARM       KWD(CCSID) TYPE(*DEC) LEN(5) DFT(*SAME) +
                          RANGE(1 65535) SPCVAL((*SAME -1) (*JOB 0) +
                          (*HEX 65535)) PROMPT('Coded character set ID')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Description')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*DEC) LEN(10) DFT(*SAME) +
                          RANGE(1 2147483646) SPCVAL((*SAME -1)) +
                          PROMPT('Initial records')
                        ELEM       TYPE(*DEC) LEN(5) DFT(*SAME) +
                          RANGE(0 32767) SPCVAL((*SAME -1)) +
                          PROMPT('Increment records')
                        ELEM       TYPE(*DEC) LEN(5) DFT(*SAME) +
                          RANGE(0 32767) SPCVAL((*SAME -1)) +
                          PROMPT('Maximum increments')
