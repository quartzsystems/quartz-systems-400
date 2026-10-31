/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SAVSYS: save the Licensed Internal Code, the operating system, the */
/* security data and the configuration as install media. */
             CMD        PROMPT('Save System') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSRCMD) HLPID(SAVSYS)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) MAX(4) +
                          PROMPT('Device')
             PARM       KWD(VOL) TYPE(*CHAR) LEN(6) MAX(75) +
                          DFT(*MOUNTED) SNGVAL((*MOUNTED)) +
                          PROMPT('Volume identifier')
             PARM       KWD(EXPDATE) TYPE(*DATE) DFT(*PERM) +
                          SPCVAL((*PERM)) PROMPT('File expiration date')
             PARM       KWD(ENDOPT) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*REWIND) VALUES(*REWIND *LEAVE *UNLOAD) +
                          PROMPT('End of media option')
             PARM       KWD(USEOPTBLK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Use optimum block size')
             PARM       KWD(OMIT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *CFG *SECDTA +
                          *DLRLS) MAX(2) PROMPT('Omit')
             PARM       KWD(CLEAR) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL *AFTER) +
                          PMTCTL(*PMTRQS) PROMPT('Clear')
             PARM       KWD(DTACPR) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*DEV) VALUES(*DEV *NO *YES *LOW *MEDIUM +
                          *HIGH) PMTCTL(*PMTRQS) PROMPT('Data compression')
             PARM       KWD(COMPACT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*DEV) VALUES(*DEV *NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Data compaction')
