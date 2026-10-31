/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CPYFRMTAP: copy the records of a tape file to a database member or */
/* to a listing.                                                       */
             CMD        PROMPT('Copy From Tape') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHSVCMD) HLPID(CPYFRMTAP)
             PARM       KWD(FROMFILE) TYPE(Q1) MIN(1) PROMPT('Tape file')
             PARM       KWD(TOFILE) TYPE(Q1) MIN(1) SNGVAL((*PRINT)) +
                          PROMPT('To file')
             PARM       KWD(FROMSEQNBR) TYPE(*INT4) DFT(*TAPF) +
                          RANGE(1 16777215) SPCVAL((*TAPF 0)) +
                          PROMPT('Sequence number')
             PARM       KWD(FROMLABEL) TYPE(*CHAR) LEN(17) DFT(*TAPF) +
                          SPCVAL((*TAPF) (*NONE)) PROMPT('Tape label')
             PARM       KWD(TOMBR) TYPE(*NAME) LEN(10) DFT(*FROMLABEL) +
                          SPCVAL((*FROMLABEL) (*FIRST)) PROMPT('Member')
             PARM       KWD(FROMDEV) TYPE(*NAME) LEN(10) DFT(*TAPF) +
                          SNGVAL((*TAPF)) MAX(4) PROMPT('Device')
             PARM       KWD(FROMREELS) TYPE(E1) PROMPT('Copy from reels')
             PARM       KWD(FROMRCDLEN) TYPE(*INT4) DFT(*TAPF) +
                          RANGE(1 32767) SPCVAL((*TAPF 0) (*CALC -1)) +
                          PROMPT('Record length')
             PARM       KWD(FROMENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*TAPF) VALUES(*TAPF *REWIND *LEAVE *UNLOAD) +
                          PROMPT('End of tape option')
             PARM       KWD(MBROPT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ADD *REPLACE) +
                          PROMPT('Replace or add records')
             PARM       KWD(OUTFMT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*CHAR) VALUES(*CHAR *HEX) +
                          PROMPT('Print format')
             PARM       KWD(FROMVOL) TYPE(*CHAR) LEN(6) DFT(*TAPF) +
                          SNGVAL((*TAPF)) MAX(50) PMTCTL(*PMTRQS) +
                          PROMPT('Volume identifier')
             PARM       KWD(FROMBLKLEN) TYPE(*INT4) DFT(*TAPF) +
                          RANGE(1 262144) SPCVAL((*TAPF 0) (*CALC -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Block length')
             PARM       KWD(FROMRCDBLK) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*TAPF) VALUES(*TAPF *F *FB *V *VB *U) +
                          PMTCTL(*PMTRQS) PROMPT('Record block type')
             PARM       KWD(NBRRCDS) TYPE(*DEC) LEN(10) DFT(*END) +
                          REL(*GT 0) SPCVAL((*END 0)) PMTCTL(*PMTRQS) +
                          PROMPT('Number of records to copy')
             E1:        ELEM       TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*TAPF) VALUES(*TAPF *SL *NL) +
                          PROMPT('Label processing type')
                        ELEM       TYPE(*INT2) DFT(*TAPF) RANGE(1 255) +
                          SPCVAL((*TAPF 0)) PROMPT('Number of reels')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
