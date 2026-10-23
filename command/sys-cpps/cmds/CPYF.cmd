/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CPYF: copy records between files, with selection, mapping and */
/* printing.                                                      */
             CMD        PROMPT('Copy File') MAXPOS(5) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CPYF)
             PARM       KWD(FROMFILE) TYPE(Q1) MIN(1) PROMPT('From file')
             PARM       KWD(TOFILE) TYPE(Q2) MIN(1) SNGVAL((*PRINT)) +
                          PROMPT('To file')
             PARM       KWD(FROMMBR) TYPE(*GENERIC) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST) (*ALL)) PROMPT('From member')
             PARM       KWD(TOMBR) TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST) (*FROMMBR) (*ALL)) +
                          PROMPT('To member or label')
             PARM       KWD(MBROPT) TYPE(*NAME) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ADD *REPLACE *UPDADD) +
                          PROMPT('Replace or add records')
             PARM       KWD(CRTFILE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PROMPT('Create file')
             PARM       KWD(OUTFMT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*CHAR) VALUES(*CHAR *HEX) +
                          PMTCTL(*PMTRQS) PROMPT('Print format')
             PARM       KWD(PRINT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*EXCLD) VALUES(*EXCLD *COPIED *ERROR) +
                          MAX(3) PMTCTL(*PMTRQS) PROMPT('Print records')
             PARM       KWD(RCDFMT) TYPE(*NAME) LEN(10) DFT(*ONLY) +
                          SPCVAL((*ONLY) (*ALL)) PMTCTL(*PMTRQS) +
                          PROMPT('Record format of logical file')
             PARM       KWD(FROMRCD) TYPE(*DEC) LEN(10) DFT(*START) +
                          REL(*GT 0) SPCVAL((*START 0)) +
                          PMTCTL(*PMTRQS) PROMPT('From record number')
             PARM       KWD(TORCD) TYPE(*DEC) LEN(10) DFT(*END) +
                          REL(*GT 0) SPCVAL((*END 0)) +
                          PMTCTL(*PMTRQS) PROMPT('To record number')
             PARM       KWD(FROMKEY) TYPE(E1) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('From record key')
             PARM       KWD(TOKEY) TYPE(E1) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('To record key')
             PARM       KWD(NBRRCDS) TYPE(*DEC) LEN(10) DFT(*END) +
                          REL(*GT 0) SPCVAL((*END 0)) +
                          PMTCTL(*PMTRQS) PROMPT('Number of records to copy')
             PARM       KWD(INCCHAR) TYPE(E2) DFT(*NONE) SNGVAL((*NONE)) +
                          PMTCTL(*PMTRQS) PROMPT('Include by character test')
             PARM       KWD(INCREL) TYPE(E3) MAX(50) DFT(*NONE) +
                          SNGVAL((*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('Include by field test')
             PARM       KWD(FMTOPT) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *NOCHK *CVTSRC *MAP +
                          *DROP *CVTFLOAT) MAX(3) PMTCTL(*PMTRQS) +
                          PROMPT('Record format field mapping')
             PARM       KWD(SRCOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SEQNBR *DATE) MAX(2) +
                          PMTCTL(*PMTRQS) PROMPT('Source update options')
             PARM       KWD(SRCSEQ) TYPE(E4) PMTCTL(*PMTRQS) +
                          PROMPT('Source sequence numbering')
             PARM       KWD(ERRLVL) TYPE(*DEC) LEN(10) DFT(0) +
                          RANGE(0 4294967288) SPCVAL((*NOMAX 4294967289)) +
                          PMTCTL(*PMTRQS) PROMPT('Errors allowed')
             PARM       KWD(COMPRESS) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Compress out deleted records')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*DEC) LEN(3) DFT(*BLDKEY) +
                          RANGE(1 120) SPCVAL((*BLDKEY 0)) +
                          PROMPT('Number of key fields')
                        ELEM       TYPE(*CHAR) LEN(256) MAX(120) +
                          PROMPT('Key value')
             E2:        ELEM       TYPE(*NAME) LEN(10) DFT(*RCD) +
                          SPCVAL((*RCD)) PROMPT('Field or *RCD')
                        ELEM       TYPE(*DEC) LEN(5) DFT(1) +
                          RANGE(1 32766) PROMPT('Starting position')
                        ELEM       TYPE(*CHAR) LEN(3) RSTD(*YES) +
                          DFT(*EQ) VALUES(*EQ *GT *LT *NE *GE *LE *NG +
                          *NL *CT) PROMPT('Relation')
                        ELEM       TYPE(*CHAR) LEN(256) MIN(1) +
                          PROMPT('Value')
             E3:        ELEM       TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*IF) VALUES(*IF *AND *OR) +
                          PROMPT('Logical operator')
                        ELEM       TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Field')
                        ELEM       TYPE(*CHAR) LEN(3) RSTD(*YES) +
                          DFT(*EQ) VALUES(*EQ *GT *LT *NE *GE *LE *NG +
                          *NL *CT) PROMPT('Relation')
                        ELEM       TYPE(*CHAR) LEN(256) MIN(1) +
                          EXPR(*YES) PROMPT('Value')
             E4:        ELEM       TYPE(*DEC) LEN(6 2) DFT(1.00) +
                          RANGE(0.01 9999.99) PROMPT('Starting number')
                        ELEM       TYPE(*DEC) LEN(6 2) DFT(1.00) +
                          RANGE(0.01 9999.99) PROMPT('Increment')
