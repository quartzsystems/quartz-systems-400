/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPTAP: display the labels, or the saves, on a tape volume.       */
             CMD        PROMPT('Display Tape') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHSVCMD) HLPID(DSPTAP)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Device')
             PARM       KWD(VOL) TYPE(*CHAR) LEN(6) DFT(*MOUNTED) +
                          SPCVAL((*MOUNTED)) PROMPT('Volume identifier')
             PARM       KWD(LABEL) TYPE(*CHAR) LEN(17) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('File label')
             PARM       KWD(SEQNBR) TYPE(E1) PROMPT('Sequence number')
             PARM       KWD(DATA) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*LABELS) VALUES(*LABELS *SAVRST *VOLLBL) +
                          PROMPT('Data type')
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *LEAVE *UNLOAD) +
                          PROMPT('End of tape option')
             E1:        ELEM       TYPE(*INT4) DFT(*FIRST) +
                          RANGE(1 16777215) SPCVAL((*FIRST 1)) +
                          PROMPT('Starting file sequence number')
                        ELEM       TYPE(*INT4) DFT(*LAST) +
                          RANGE(1 16777215) SPCVAL((*LAST 16777215) +
                          (*ONLY 0)) PROMPT('Ending file sequence number')
