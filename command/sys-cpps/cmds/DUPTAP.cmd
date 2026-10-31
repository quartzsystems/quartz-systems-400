/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DUPTAP: copy files from the tape on one device to another.        */
             CMD        PROMPT('Duplicate Tape') MAXPOS(4) ALLOW(*ALL) +
                          HLPPNLGRP(QHSVCMD) HLPID(DUPTAP)
             PARM       KWD(FROMDEV) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('From device')
             PARM       KWD(TODEV) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('To device')
             PARM       KWD(FROMSEQNBR) TYPE(E1) +
                          PROMPT('From sequence number')
             PARM       KWD(TOSEQNBR) TYPE(*INT4) DFT(*FROMSEQ) +
                          RANGE(1 16777215) SPCVAL((*FROMSEQ 0) +
                          (*END -1)) PROMPT('To sequence number')
             PARM       KWD(FROMVOL) TYPE(*CHAR) LEN(6) MAX(50) +
                          DFT(*MOUNTED) SNGVAL((*MOUNTED)) +
                          PMTCTL(*PMTRQS) PROMPT('From volume identifier')
             PARM       KWD(TOVOL) TYPE(*CHAR) LEN(6) MAX(50) +
                          DFT(*MOUNTED) SNGVAL((*MOUNTED)) +
                          PMTCTL(*PMTRQS) PROMPT('To volume identifier')
             PARM       KWD(TODENSITY) TYPE(*CHAR) LEN(10) +
                          DFT(*DEVTYPE) SPCVAL((*DEVTYPE) (*CTGTYPE)) +
                          PMTCTL(*PMTRQS) PROMPT('Tape density')
             PARM       KWD(COMPACT) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*FROMFILE) VALUES(*FROMFILE *DEVD *NO) +
                          PMTCTL(*PMTRQS) PROMPT('Data compaction')
             PARM       KWD(FILES) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *ACTIVE) +
                          PMTCTL(*PMTRQS) PROMPT('Files to duplicate')
             PARM       KWD(USRLBLPGM) TYPE(Q1) DFT(*SYSCOPY) +
                          SNGVAL((*SYSCOPY) (*NONE)) PMTCTL(*PMTRQS) +
                          PROMPT('User label program')
             PARM       KWD(FROMENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *LEAVE *UNLOAD) +
                          PMTCTL(*PMTRQS) PROMPT('From device end option')
             PARM       KWD(TOENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*UNLOAD) VALUES(*REWIND *LEAVE *UNLOAD) +
                          PMTCTL(*PMTRQS) PROMPT('To device end option')
             PARM       KWD(CHECK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(*PMTRQS) +
                          PROMPT('Check for active files')
             PARM       KWD(EXPDATE) TYPE(*DATE) DFT(*FROMFILE) +
                          SPCVAL((*FROMFILE) (*PERM)) PMTCTL(*PMTRQS) +
                          PROMPT('File expiration date')
             E1:        ELEM       TYPE(*INT4) DFT(*FIRST) +
                          RANGE(1 16777215) SPCVAL((*FIRST 1) (*ALL 0)) +
                          PROMPT('Starting file sequence number')
                        ELEM       TYPE(*INT4) DFT(*LAST) +
                          RANGE(1 16777215) SPCVAL((*LAST 16777215) +
                          (*ONLY -1)) PROMPT('Ending file sequence number')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
