/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SAVCFG: save the configuration objects. */
             CMD        PROMPT('Save Configuration') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSRCMD) HLPID(SAVCFG)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) MAX(4) +
                          SNGVAL((*SAVF) (*MEDDFN)) PROMPT('Device')
             PARM       KWD(VOL) TYPE(*CHAR) LEN(6) MAX(75) +
                          DFT(*MOUNTED) SNGVAL((*MOUNTED)) +
                          PMTCTL(P1) PROMPT('Volume identifier')
             PARM       KWD(SEQNBR) TYPE(*INT4) DFT(*END) +
                          RANGE(1 16777215) SPCVAL((*END 0)) +
                          PMTCTL(P1) PROMPT('Sequence number')
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(17) DFT(*SAVCFG) +
                          SPCVAL((*SAVCFG)) PMTCTL(P1) PROMPT('File label')
             PARM       KWD(EXPDATE) TYPE(*DATE) DFT(*PERM) +
                          SPCVAL((*PERM)) PMTCTL(P1) +
                          PROMPT('File expiration date')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *LEAVE *UNLOAD) +
                          PMTCTL(P1) PROMPT('End of media option')
             PARM       KWD(SAVF) TYPE(Q1) PMTCTL(P2) +
                          PROMPT('Save file')
             PARM       KWD(MEDDFN) TYPE(Q1) PMTCTL(P3) +
                          PROMPT('Media definition')
             PARM       KWD(OPTFILE) TYPE(*PNAME) LEN(256) DFT('*') +
                          CASE(*MIXED) PMTCTL(P1) PROMPT('Optical file')
             PARM       KWD(USEOPTBLK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PMTCTL(P1) +
                          PROMPT('Use optimum block size')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *PRINT *OUTFILE) +
                          PMTCTL(*PMTRQS) PROMPT('Output')
             PARM       KWD(OUTFILE) TYPE(Q1) PMTCTL(*PMTRQS) +
                          PROMPT('File to receive output')
             PARM       KWD(OUTMBR) TYPE(E2) PMTCTL(*PMTRQS) +
                          PROMPT('Output member options')
             PARM       KWD(CLEAR) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL *AFTER +
                          *REPLACE) PMTCTL(*PMTRQS) PROMPT('Clear')
             PARM       KWD(DTACPR) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*DEV) VALUES(*DEV *NO *YES *LOW *MEDIUM +
                          *HIGH) PMTCTL(*PMTRQS) PROMPT('Data compression')
             PARM       KWD(COMPACT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*DEV) VALUES(*DEV *NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Data compaction')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E2:        ELEM       TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST)) PROMPT('Member to receive output')
                        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*REPLACE) VALUES(*REPLACE *ADD) +
                          PROMPT('Replace or add records')
             P1:        PMTCTL     CTL(DEV) COND((*NE *SAVF) +
                          (*NE *MEDDFN)) NBRTRUE(*ALL)
             P2:        PMTCTL     CTL(DEV) COND((*EQ *SAVF))
             P3:        PMTCTL     CTL(DEV) COND((*EQ *MEDDFN))
