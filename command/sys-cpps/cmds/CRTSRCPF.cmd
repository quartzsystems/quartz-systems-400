/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTSRCPF: create a source physical file. */
             CMD        PROMPT('Create Source Physical File') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CRTSRCPF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(RCDLEN) TYPE(*DEC) LEN(5) DFT(92) +
                          RANGE(13 32766) PROMPT('Record length')
             PARM       KWD(IGCDTA) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PROMPT('DBCS data')
             PARM       KWD(MBR) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE) (*FILE)) PROMPT('Member')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(SYSTEM) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*LCL) VALUES(*LCL *RMT *FILETYPE) +
                          PMTCTL(*PMTRQS) PROMPT('System')
             PARM       KWD(EXPDATE) TYPE(*DATE) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Expiration date for member')
             PARM       KWD(MAXMBRS) TYPE(*DEC) LEN(5) DFT(*NOMAX) +
                          RANGE(1 32767) SPCVAL((*NOMAX 0)) +
                          PMTCTL(*PMTRQS) PROMPT('Maximum members')
             PARM       KWD(ACCPTHSIZ) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*MAX1TB) VALUES(*MAX1TB *MAX4GB) +
                          PMTCTL(*PMTRQS) PROMPT('Access path size')
             PARM       KWD(SIZE) TYPE(E1) DFT(10000) SNGVAL((*NOMAX)) +
                          PMTCTL(*PMTRQS) PROMPT('Member size')
             PARM       KWD(ALLOCATE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Allocate storage')
             PARM       KWD(CONTIG) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Contiguous storage')
             PARM       KWD(UNIT) TYPE(*DEC) LEN(3) DFT(*ANY) +
                          RANGE(1 255) SPCVAL((*ANY 0)) PMTCTL(*PMTRQS) +
                          PROMPT('Preferred storage unit')
             PARM       KWD(FRCRATIO) TYPE(*DEC) LEN(5) DFT(*NONE) +
                          RANGE(1 32767) SPCVAL((*NONE 0)) +
                          PMTCTL(*PMTRQS) PROMPT('Records to force a write')
             PARM       KWD(WAITFILE) TYPE(*DEC) LEN(5) DFT(*IMMED) +
                          RANGE(1 32767) SPCVAL((*IMMED 0) (*CLS -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Maximum file wait time')
             PARM       KWD(WAITRCD) TYPE(*DEC) LEN(5) DFT(60) +
                          RANGE(1 32767) SPCVAL((*IMMED 0) (*NOMAX -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Maximum record wait time')
             PARM       KWD(SHARE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Share open data path')
             PARM       KWD(DLTPCT) TYPE(*DEC) LEN(3) DFT(*NONE) +
                          RANGE(1 100) SPCVAL((*NONE 0)) PMTCTL(*PMTRQS) +
                          PROMPT('Max % deleted records allowed')
             PARM       KWD(CCSID) TYPE(*DEC) LEN(5) DFT(*JOB) +
                          RANGE(1 65535) SPCVAL((*JOB 0) (*HEX 65535)) +
                          PMTCTL(*PMTRQS) PROMPT('Coded character set ID')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) +
                          PROMPT('Public authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*DEC) LEN(10) DFT(10000) +
                          RANGE(1 2147483646) PROMPT('Initial records')
                        ELEM       TYPE(*DEC) LEN(5) DFT(1000) +
                          RANGE(0 32767) PROMPT('Increment records')
                        ELEM       TYPE(*DEC) LEN(5) DFT(499) +
                          RANGE(0 32767) PROMPT('Maximum increments')
