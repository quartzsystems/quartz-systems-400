/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGTAPF: change the attributes of a tape file.                     */
             CMD        PROMPT('Change Tape File') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHSVCMD) HLPID(CHGTAPF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) MAX(4) +
                          PROMPT('Tape device')
             PARM       KWD(VOL) TYPE(*CHAR) LEN(6) DFT(*SAME) +
                          SNGVAL((*SAME) (*NONE)) MAX(50) +
                          PROMPT('Volume identifier')
             PARM       KWD(REELS) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *SL *NL) +
                          PROMPT('Tape reels')
             PARM       KWD(SEQNBR) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 16777215) SPCVAL((*SAME 0) (*END -1)) +
                          PROMPT('Sequence number')
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(17) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE)) PROMPT('Tape label')
             PARM       KWD(RCDLEN) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 32767) SPCVAL((*SAME -1) (*CALC 0)) +
                          PROMPT('Record length')
             PARM       KWD(BLKLEN) TYPE(*INT4) DFT(*SAME) +
                          RANGE(1 262144) SPCVAL((*SAME -1) (*CALC 0)) +
                          PROMPT('Block length')
             PARM       KWD(RCDBLKTYP) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *F *FB *V *VB *U) +
                          PROMPT('Record block type')
             PARM       KWD(CODE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *EBCDIC *ASCII) +
                          PROMPT('Code')
             PARM       KWD(EXPDATE) TYPE(*DATE) DFT(*SAME) +
                          SPCVAL((*SAME) (*NONE) (*PERM)) +
                          PROMPT('File expiration date')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *REWIND *LEAVE *UNLOAD) +
                          PROMPT('End of tape option')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Text ''description''')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
