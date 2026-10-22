/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RGZPFM: reorganize a physical file member, dropping deleted records */
/* and optionally reordering by a key.                                 */
             CMD        PROMPT('Reorganize Physical File Mbr') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(RGZPFM)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(MBR) TYPE(*NAME) LEN(10) DFT(*FIRST) +
                          SPCVAL((*FIRST) (*LAST)) PROMPT('Member')
             PARM       KWD(KEYFILE) TYPE(Q2) DFT(*NONE) SNGVAL((*NONE) +
                          (*FILE) (*RPLDLTRCD)) PROMPT('Key file')
             PARM       KWD(SRCOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SEQNBR) MAX(2) +
                          PROMPT('Source update options')
             PARM       KWD(SRCSEQ) TYPE(E1) PROMPT('Source sequence numbering')
             PARM       KWD(ALWCANCEL) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Allow cancel')
             PARM       KWD(LOCK) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*EXCL) VALUES(*EXCL *EXCLRD *SHRUPD) +
                          PROMPT('Lock state')
             PARM       KWD(RBDACCPTH) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO *OPTIMIZE) +
                          PROMPT('Rebuild access paths')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             Q2:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
             E1:        ELEM       TYPE(*DEC) LEN(6 2) DFT(1.00) +
                          RANGE(0.01 9999.99) PROMPT('Starting number')
                        ELEM       TYPE(*DEC) LEN(6 2) DFT(1.00) +
                          RANGE(0.01 9999.99) PROMPT('Increment')
