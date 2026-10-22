/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGPF: change a physical file's attributes, or its format from new */
/* DDS source.                                                         */
             CMD        PROMPT('Change Physical File') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CHGPF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(SRCFILE) TYPE(Q2) DFT(*NONE) SNGVAL((*NONE)) +
                          PROMPT('Source file')
             PARM       KWD(SRCMBR) TYPE(*NAME) LEN(10) DFT(*FILE) +
                          SPCVAL((*FILE)) PROMPT('Source member')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SRC) VALUES(*SRC *SOURCE *NOSRC +
                          *NOSOURCE *LIST *NOLIST *SECLVL *NOSECLVL +
                          *EVENTF *NOEVENTF) MAX(4) +
                          PROMPT('Source listing options')
             PARM       KWD(GENLVL) TYPE(*INT2) DFT(20) RANGE(0 30) +
                          PROMPT('Generation severity level')
             PARM       KWD(FLAG) TYPE(*INT2) DFT(0) RANGE(0 30) +
                          PROMPT('Flagging severity level')
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
             PARM       KWD(PAGESIZE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *KEYLEN 8 16 32 64 +
                          128 256 512) +
                          PROMPT('Access path logical page size')
             PARM       KWD(MAINT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *IMMED *REBLD *DLY) +
                          PROMPT('Access path maintenance')
             PARM       KWD(RECOVER) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *AFTIPL *IPL) +
                          PROMPT('Access path recovery')
             PARM       KWD(FRCACCPTH) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Force keyed access path')
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
             PARM       KWD(REUSEDLT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Reuse deleted records')
             PARM       KWD(SRTSEQ) TYPE(Q2) DFT(*SAME) SNGVAL((*SAME) +
                          (*SRC) (*JOB) (*LANGIDSHR) (*LANGIDUNQ) (*HEX)) +
                          PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*NAME) LEN(3) DFT(*SAME) +
                          SPCVAL((*SAME) (*JOB) (*JOBRUN)) +
                          PROMPT('Language identifier')
             PARM       KWD(CCSID) TYPE(*DEC) LEN(5) DFT(*SAME) +
                          RANGE(1 65535) SPCVAL((*SAME -1) (*JOB 0) +
                          (*HEX 65535)) PROMPT('Coded character set ID')
             PARM       KWD(ALWUPD) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Allow update operation')
             PARM       KWD(ALWDLT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Allow delete operation')
             PARM       KWD(LVLCHK) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Record format level check')
             PARM       KWD(NODGRP) TYPE(Q2) DFT(*SAME) SNGVAL((*SAME) +
                          (*NONE)) PROMPT('Node group')
             PARM       KWD(PTNKEY) TYPE(*NAME) LEN(10) MAX(300) +
                          DFT(*SAME) SNGVAL((*SAME) (*NONE)) +
                          PROMPT('Partitioning key')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Description')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
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
