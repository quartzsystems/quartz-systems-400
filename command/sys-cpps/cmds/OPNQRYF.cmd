/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* OPNQRYF: open a query over one or more files with selection, */
/* ordering, joins, grouping and mapped fields.                   */
             CMD        PROMPT('Open Query File') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(OPNQRYF)
             PARM       KWD(FILE) TYPE(E1) MIN(1) MAX(32) PROMPT('File')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*INP) VALUES(*INP *OUT *UPD *DLT *ALL) +
                          MAX(4) PROMPT('Open options')
             PARM       KWD(FORMAT) TYPE(Q2) DFT(*FILE) SNGVAL((*FILE)) +
                          PROMPT('Format of open records')
             PARM       KWD(QRYSLT) TYPE(*CHAR) LEN(2000) DFT(*ALL) +
                          EXPR(*YES) PROMPT('Query selection')
             PARM       KWD(KEYFLD) TYPE(E2) MAX(50) DFT(*NONE) +
                          SNGVAL((*NONE) (*FILE)) PROMPT('Key field')
             PARM       KWD(UNIQUEKEY) TYPE(*DEC) LEN(3) DFT(*NONE) +
                          RANGE(1 120) SPCVAL((*NONE 0) (*ALL -1)) +
                          PROMPT('Unique key fields')
             PARM       KWD(JFLD) TYPE(E3) MAX(50) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Join field')
             PARM       KWD(JDFTVAL) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES *ONLYDFT) +
                          PROMPT('Join with default values')
             PARM       KWD(JORDER) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*ANY) VALUES(*ANY *FILE) +
                          PROMPT('Join file order')
             PARM       KWD(GRPFLD) TYPE(*CHAR) LEN(21) MAX(50) +
                          DFT(*NONE) SNGVAL((*NONE)) PROMPT('Group field')
             PARM       KWD(GRPSLT) TYPE(*CHAR) LEN(2000) DFT(*ALL) +
                          EXPR(*YES) PROMPT('Group selection')
             PARM       KWD(MAPFLD) TYPE(E4) MAX(50) DFT(*NONE) +
                          SNGVAL((*NONE)) PROMPT('Mapped field')
             PARM       KWD(IGNDECERR) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Ignore decimal data errors')
             PARM       KWD(OPNID) TYPE(*NAME) LEN(10) DFT(*FILE) +
                          SPCVAL((*FILE)) PROMPT('Open file identifier')
             PARM       KWD(SEQONLY) TYPE(E5) PMTCTL(*PMTRQS) +
                          PROMPT('Limit to sequential only')
             PARM       KWD(COMMIT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Commitment control active')
             PARM       KWD(OPNSCOPE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*ACTGRPDFN) VALUES(*ACTGRPDFN *JOB) +
                          PMTCTL(*PMTRQS) PROMPT('Open scope')
             PARM       KWD(DUPKEYCHK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Duplicate key check')
             PARM       KWD(ALWCPYDTA) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO *OPTIMIZE) +
                          PMTCTL(*PMTRQS) PROMPT('Allow copy data')
             PARM       KWD(OPTIMIZE) TYPE(*DEC) LEN(10) DFT(*ALLIO) +
                          RANGE(1 2147483647) SPCVAL((*ALLIO 0) +
                          (*FIRSTIO -1) (*MINWAIT -2)) PMTCTL(*PMTRQS) +
                          PROMPT('Performance optimization')
             PARM       KWD(OPTALLAP) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Optimize all access paths')
             PARM       KWD(SRTSEQ) TYPE(Q2) DFT(*JOB) SNGVAL((*JOB) +
                          (*LANGIDSHR) (*LANGIDUNQ) (*HEX)) +
                          PMTCTL(*PMTRQS) PROMPT('Sort sequence')
             PARM       KWD(LANGID) TYPE(*NAME) LEN(3) DFT(*JOB) +
                          SPCVAL((*JOB) (*JOBRUN)) PMTCTL(*PMTRQS) +
                          PROMPT('Language identifier')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*NORMAL) VALUES(*NORMAL *PERM) +
                          PMTCTL(*PMTRQS) PROMPT('Type of open')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(Q1) MIN(1) PROMPT('File')
                        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member')
                        ELEM       TYPE(*NAME) LEN(10) DFT(*ONLY) +
                          SPCVAL((*ONLY)) PROMPT('Record format')
             E2:        ELEM       TYPE(*CHAR) LEN(21) MIN(1) +
                          PROMPT('Field')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*ASCEND) VALUES(*ASCEND *DESCEND) +
                          PROMPT('Order')
                        ELEM       TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SIGNED) VALUES(*SIGNED *ABSVAL) +
                          PROMPT('Absolute value')
             E3:        ELEM       TYPE(*CHAR) LEN(21) MIN(1) +
                          PROMPT('From field')
                        ELEM       TYPE(*CHAR) LEN(21) MIN(1) +
                          PROMPT('To field')
                        ELEM       TYPE(*CHAR) LEN(3) RSTD(*YES) +
                          DFT(*EQ) VALUES(*EQ *GT *LT *NE *GE *LE *NG +
                          *NL) PROMPT('Join operator')
             E4:        ELEM       TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Mapped field')
                        ELEM       TYPE(*CHAR) LEN(256) MIN(1) +
                          EXPR(*YES) PROMPT('Expression')
                        ELEM       TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*CALC) VALUES(*CALC *CHAR *HEX *ZONED +
                          *PACKED *BIN *FLT *DATE *TIME *TIMESTP *VCHAR +
                          *VHEX *GRAPHIC *VGRAPHIC) PROMPT('Type')
                        ELEM       TYPE(*DEC) LEN(5) DFT(*CALC) +
                          RANGE(1 32766) SPCVAL((*CALC 0)) +
                          PROMPT('Length')
                        ELEM       TYPE(*DEC) LEN(2) DFT(*CALC) +
                          RANGE(0 63) SPCVAL((*CALC -1)) +
                          PROMPT('Decimal positions')
                        ELEM       TYPE(*DEC) LEN(5) DFT(*CALC) +
                          RANGE(1 65535) SPCVAL((*CALC 0)) +
                          PROMPT('CCSID')
             E5:        ELEM       TYPE(*CHAR) LEN(4) RSTD(*YES) MIN(1) +
                          VALUES(*NO *YES) PROMPT('Sequential only')
                        ELEM       TYPE(*DEC) LEN(5) RANGE(1 32767) +
                          PROMPT('Records per buffer')
