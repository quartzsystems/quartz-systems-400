/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTPF: create a physical file from DDS source or a record length. */
             CMD        PROMPT('Create Physical File') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CRTPF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(RCDLEN) TYPE(*DEC) LEN(5) RANGE(1 32766) +
                          PROMPT('Record length')
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
             PARM       KWD(MBR) TYPE(*NAME) LEN(10) DFT(*FILE) +
                          SPCVAL((*FILE) (*NONE)) PROMPT('Member')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SRC) VALUES(*SRC *SOURCE *NOSRC +
                          *NOSOURCE *LIST *NOLIST *SECLVL *NOSECLVL +
                          *EVENTF *NOEVENTF) MAX(4) PMTCTL(*PMTRQS) +
                          PROMPT('Source listing options')
             PARM       KWD(SYSTEM) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*LCL) VALUES(*LCL *RMT *FILETYPE) +
                          PMTCTL(*PMTRQS) PROMPT('System')
             PARM       KWD(EXPDATE) TYPE(*DATE) DFT(*NONE) +
                          SPCVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Expiration date for member')
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
             PARM       KWD(REUSEDLT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Reuse deleted records')
             PARM       KWD(SRTSEQ) TYPE(Q3) DFT(*SRC) SNGVAL((*SRC) +
                          (*JOB) (*LANGIDSHR) (*LANGIDUNQ) (*HEX)) +
                          PMTCTL(*PMTRQS) PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*NAME) LEN(3) DFT(*JOB) +
                          SPCVAL((*JOB) (*JOBRUN)) PMTCTL(*PMTRQS) +
                          PROMPT('Language identifier')
             PARM       KWD(CCSID) TYPE(*DEC) LEN(5) DFT(*JOB) +
                          RANGE(1 65535) SPCVAL((*JOB 0) (*HEX 65535)) +
                          PMTCTL(*PMTRQS) PROMPT('Coded character set ID')
             PARM       KWD(ALWUPD) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Allow update operation')
             PARM       KWD(ALWDLT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Allow delete operation')
             PARM       KWD(LVLCHK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Record format level check')
             PARM       KWD(NODGRP) TYPE(Q2) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Node group')
             PARM       KWD(PTNKEY) TYPE(*NAME) LEN(10) MAX(300) +
                          DFT(*NONE) SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Partitioning key')
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
             Q3:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*DEC) LEN(10) DFT(10000) +
                          RANGE(1 2147483646) PROMPT('Initial records')
                        ELEM       TYPE(*DEC) LEN(5) DFT(1000) +
                          RANGE(0 32767) PROMPT('Increment records')
                        ELEM       TYPE(*DEC) LEN(5) DFT(3) +
                          RANGE(0 32767) PROMPT('Maximum increments')
