/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTLF: create a logical file over physical files from DDS source. */
             CMD        PROMPT('Create Logical File') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CRTLF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(SRCFILE) TYPE(Q2) DFT(QDDSSRC) +
                          PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*FILE) +
                          SPCVAL((*FILE)) PROMPT('Source member')
             PARM       KWD(GENLVL) TYPE(*INT2) DFT(20) RANGE(0 30) +
                          PROMPT('Generation severity level')
             PARM       KWD(FLAG) TYPE(*INT2) DFT(0) RANGE(0 30) +
                          PROMPT('Flagging severity level')
             PARM       KWD(FILETYPE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*DATA) VALUES(*DATA *SRC) +
                          PROMPT('File type')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SRC) VALUES(*SRC *SOURCE *NOSRC +
                          *NOSOURCE *LIST *NOLIST *SECLVL *NOSECLVL +
                          *EVENTF *NOEVENTF) MAX(4) +
                          PROMPT('Source listing options')
             PARM       KWD(SYSTEM) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*LCL) VALUES(*LCL *RMT *FILETYPE) +
                          PROMPT('System')
             PARM       KWD(DTAMBRS) TYPE(E1) MAX(32) DFT(*ALL) +
                          SNGVAL((*ALL)) PROMPT('Physical file data members')
             PARM       KWD(MBR) TYPE(*NAME) LEN(10) DFT(*FILE) +
                          SPCVAL((*FILE) (*NONE)) PROMPT('Member')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(MAXMBRS) TYPE(*DEC) LEN(5) DFT(1) +
                          RANGE(1 32767) SPCVAL((*NOMAX 0)) +
                          PMTCTL(*PMTRQS) PROMPT('Maximum members')
             PARM       KWD(ACCPTHSIZ) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*MAX1TB) VALUES(*MAX1TB *MAX4GB) +
                          PMTCTL(*PMTRQS) PROMPT('Access path size')
             PARM       KWD(PAGESIZE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*KEYLEN) VALUES(*KEYLEN 8 16 32 64 128 +
                          256 512) PMTCTL(*PMTRQS) +
                          PROMPT('Access path logical page size')
             PARM       KWD(MAINT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*IMMED) VALUES(*IMMED *REBLD *DLY) +
                          PMTCTL(*PMTRQS) PROMPT('Access path maintenance')
             PARM       KWD(RECOVER) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *AFTIPL *IPL) +
                          PMTCTL(*PMTRQS) PROMPT('Access path recovery')
             PARM       KWD(FRCACCPTH) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Force keyed access path')
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
             PARM       KWD(SRTSEQ) TYPE(Q2) DFT(*SRC) SNGVAL((*SRC) +
                          (*JOB) (*LANGIDSHR) (*LANGIDUNQ) (*HEX)) +
                          PMTCTL(*PMTRQS) PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*NAME) LEN(3) DFT(*JOB) +
                          SPCVAL((*JOB) (*JOBRUN)) PMTCTL(*PMTRQS) +
                          PROMPT('Language identifier')
             PARM       KWD(LVLCHK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Record format level check')
             PARM       KWD(AUT) TYPE(*NAME) LEN(10) DFT(*LIBCRTAUT) +
                          SPCVAL((*LIBCRTAUT) (*CHANGE) (*ALL) (*USE) +
                          (*EXCLUDE)) PMTCTL(*PMTRQS) +
                          PROMPT('Public authority')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q3:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURRENT) +
                          SPCVAL((*CURRENT) (*LIBL) (*CURLIB)) +
                          PROMPT('Library')
             E1:        ELEM       TYPE(Q3) MIN(1) PROMPT('Physical file')
                        ELEM       TYPE(*NAME) LEN(10) MAX(32) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Members')
