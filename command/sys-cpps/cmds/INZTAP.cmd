/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* INZTAP: initialize the tape on a device, labeled or not.          */
             CMD        PROMPT('Initialize Tape') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) HLPID(INZTAP)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Device')
             PARM       KWD(NEWVOL) TYPE(*CHAR) LEN(6) DFT(*NONE) +
                          SPCVAL((*NONE) (*CTGID)) +
                          PROMPT('New volume identifier')
             PARM       KWD(NEWOWNID) TYPE(*CHAR) LEN(14) +
                          DFT(*BLANK) SPCVAL((*BLANK ' ')) +
                          PROMPT('New owner identifier')
             PARM       KWD(VOL) TYPE(*CHAR) LEN(6) DFT(*MOUNTED) +
                          SPCVAL((*MOUNTED)) PROMPT('Volume identifier')
             PARM       KWD(CHECK) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO *FIRST) +
                          PROMPT('Check for active files')
             PARM       KWD(DENSITY) TYPE(*CHAR) LEN(10) +
                          DFT(*DEVTYPE) SPCVAL((*DEVTYPE) (*CTGTYPE)) +
                          PROMPT('Tape density')
             PARM       KWD(CODE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*EBCDIC) VALUES(*EBCDIC *ASCII) +
                          PROMPT('Code')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *UNLOAD) +
                          PROMPT('End of tape option')
             PARM       KWD(CLEAR) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*YES *NO) PROMPT('Clear')
