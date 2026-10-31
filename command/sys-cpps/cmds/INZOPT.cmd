/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* INZOPT: initialize an optical volume.                              */
             CMD        PROMPT('Initialize Optical') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHOPCMD) HLPID(INZOPT)
             PARM       KWD(VOL) TYPE(*CHAR) LEN(32) DFT(*MOUNTED) +
                          SPCVAL((*MOUNTED)) KEYPARM(*YES) +
                          PROMPT('Volume identifier')
             PARM       KWD(NEWVOL) TYPE(*CHAR) LEN(32) DFT(*VOL) +
                          SPCVAL((*VOL)) PROMPT('New volume identifier')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) PROMPT('Device')
             PARM       KWD(THRESHOLD) TYPE(*INT2) DFT(*CALC) +
                          RANGE(1 99) SPCVAL((*CALC 0)) +
                          PROMPT('Volume full threshold')
             PARM       KWD(CHECK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Check for an active volume')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*LEAVE) VALUES(*LEAVE *UNLOAD) +
                          PROMPT('End of media option')
             PARM       KWD(CLEAR) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*YES *NO) PROMPT('Clear')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) +
                          PROMPT('Text ''description''')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*PRIMARY) VALUES(*PRIMARY *BACKUP) +
                          PMTCTL(*PMTRQS) PROMPT('Volume type')
             PARM       KWD(CCSID) TYPE(*INT4) DFT(*CALC) +
                          SPCVAL((*CALC 0)) PMTCTL(*PMTRQS) +
                          PROMPT('Coded character set ID')
             PARM       KWD(MEDFMT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*MEDTYPE) VALUES(*MEDTYPE *UDF) +
                          PMTCTL(*PMTRQS) PROMPT('Media format')
