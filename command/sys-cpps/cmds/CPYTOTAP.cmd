/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CPYTOTAP: copy the records of a database member to a tape file.    */
             CMD        PROMPT('Copy To Tape') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHSVCMD) HLPID(CPYTOTAP)
             PARM       KWD(FROMFILE) TYPE(Q1) MIN(1) PROMPT('From file')
             PARM       KWD(TOFILE) TYPE(Q1) MIN(1) PROMPT('Tape file')
             PARM       KWD(FROMMBR) TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('From member')
             PARM       KWD(TOSEQNBR) TYPE(*INT4) DFT(*TAPF) +
                          RANGE(1 16777215) SPCVAL((*TAPF 0) (*END -1)) +
                          PROMPT('File sequence number')
             PARM       KWD(TOLABEL) TYPE(*CHAR) LEN(17) DFT(*FROMMBR) +
                          SPCVAL((*FROMMBR) (*TAPF) (*NONE)) +
                          PROMPT('Tape label')
             PARM       KWD(TODEV) TYPE(*NAME) LEN(10) DFT(*TAPF) +
                          SNGVAL((*TAPF)) MAX(4) PROMPT('Device')
             PARM       KWD(TOREELS) TYPE(E1) PROMPT('Copy to reels')
             PARM       KWD(TORCDLEN) TYPE(*INT4) DFT(*FROMFILE) +
                          RANGE(1 32767) SPCVAL((*FROMFILE 0) (*TAPF -1)) +
                          PROMPT('Record length')
             PARM       KWD(TOENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*TAPF) VALUES(*TAPF *REWIND *LEAVE *UNLOAD) +
                          PROMPT('End of tape option')
             PARM       KWD(TOVOL) TYPE(*CHAR) LEN(6) DFT(*TAPF) +
                          SNGVAL((*TAPF)) MAX(50) PMTCTL(*PMTRQS) +
                          PROMPT('Volume identifier')
             PARM       KWD(TOBLKLEN) TYPE(*INT4) DFT(*TAPF) +
                          RANGE(1 262144) SPCVAL((*TAPF 0) (*CALC -1)) +
                          PMTCTL(*PMTRQS) PROMPT('Block length')
             PARM       KWD(TORCDBLK) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*TAPF) VALUES(*TAPF *F *FB *V *VB *U) +
                          PMTCTL(*PMTRQS) PROMPT('Record block type')
             PARM       KWD(TOEXPDATE) TYPE(*DATE) DFT(*TAPF) +
                          SPCVAL((*TAPF) (*PERM)) PMTCTL(*PMTRQS) +
                          PROMPT('File expiration date')
             PARM       KWD(NBRRCDS) TYPE(*DEC) LEN(10) DFT(*END) +
                          REL(*GT 0) SPCVAL((*END 0)) PMTCTL(*PMTRQS) +
                          PROMPT('Number of records to copy')
             PARM       KWD(COMPACT) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*TAPF) VALUES(*TAPF *DEVD *NO) +
                          PMTCTL(*PMTRQS) PROMPT('Data compaction')
             E1:        ELEM       TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*TAPF) VALUES(*TAPF *SL *NL) +
                          PROMPT('Label processing type')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
