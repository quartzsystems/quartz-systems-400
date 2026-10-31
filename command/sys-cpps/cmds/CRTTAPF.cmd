/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTTAPF: create a tape file (*FILE TAPF): the device, volumes,     */
/* labels and record layout CPYTOTAP and CPYFRMTAP take by default.    */
             CMD        PROMPT('Create Tape File') MAXPOS(1) ALLOW(*ALL) +
                          HLPPNLGRP(QHSVCMD) HLPID(CRTTAPF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SNGVAL((*NONE)) MAX(4) PROMPT('Tape device')
             PARM       KWD(VOL) TYPE(*CHAR) LEN(6) DFT(*NONE) +
                          SNGVAL((*NONE)) MAX(50) +
                          PROMPT('Volume identifier')
             PARM       KWD(REELS) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SL) VALUES(*SL *NL) PROMPT('Tape reels')
             PARM       KWD(SEQNBR) TYPE(*INT4) DFT(1) +
                          RANGE(1 16777215) SPCVAL((*END -1)) +
                          PROMPT('Sequence number')
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(17) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Tape label')
             PARM       KWD(RCDLEN) TYPE(*INT4) DFT(*CALC) +
                          RANGE(1 32767) SPCVAL((*CALC 0)) +
                          PROMPT('Record length')
             PARM       KWD(BLKLEN) TYPE(*INT4) DFT(*CALC) +
                          RANGE(1 262144) SPCVAL((*CALC 0)) +
                          PROMPT('Block length')
             PARM       KWD(RCDBLKTYP) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*FB) VALUES(*F *FB *V *VB *U) +
                          PROMPT('Record block type')
             PARM       KWD(CODE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*EBCDIC) VALUES(*EBCDIC *ASCII) +
                          PROMPT('Code')
             PARM       KWD(EXPDATE) TYPE(*DATE) DFT(*NONE) +
                          SPCVAL((*NONE) (*PERM)) +
                          PROMPT('File expiration date')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *LEAVE *UNLOAD) +
                          PROMPT('End of tape option')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Text ''description''')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*CURLIB) +
                          SPCVAL((*CURLIB)) PROMPT('Library')
