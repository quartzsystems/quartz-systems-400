/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHKTAP: check the volume or file on a tape device. */
             CMD        PROMPT('Check Tape') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) HLPID(CHKTAP)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Device')
             PARM       KWD(VOL) TYPE(*CHAR) LEN(6) DFT(*NONE) +
                          SPCVAL((*NONE) (*MOUNTED)) PROMPT('Volume identifier')
             PARM       KWD(SEQNBR) TYPE(*INT4) DFT(*NONE) +
                          RANGE(1 16777215) SPCVAL((*NONE 0) (*NEXT -1)) +
                          PROMPT('Sequence number')
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(17) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('File label')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *LEAVE *UNLOAD) +
                          PROMPT('End of tape option')
