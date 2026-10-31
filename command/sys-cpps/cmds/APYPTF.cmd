/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* APYPTF: apply loaded program temporary fixes. */
             CMD        PROMPT('Apply PTF') MAXPOS(2) ALLOW(*ALL) +
                          HLPPNLGRP(QHSVCMD) HLPID(APYPTF)
             PARM       KWD(LICPGM) TYPE(*CHAR) LEN(7) MIN(1) +
                          PROMPT('Product')
             PARM       KWD(SELECT) TYPE(*CHAR) LEN(7) DFT(*ALL) +
                          MAX(300) SNGVAL((*ALL)) PROMPT('PTF numbers to select')
             PARM       KWD(OMIT) TYPE(*CHAR) LEN(7) MAX(300) +
                          PROMPT('PTF numbers to omit')
             PARM       KWD(RLS) TYPE(*CHAR) LEN(6) DFT(*ONLY) +
                          SPCVAL((*ONLY)) PROMPT('Release')
             PARM       KWD(APY) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*TEMP) VALUES(*TEMP *PERM) PROMPT('Extent of change')
             PARM       KWD(DELAYED) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PROMPT('Delayed PTFs')
             PARM       KWD(IPLAPY) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Apply at unattended IPL')
             PARM       KWD(APYREQ) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) PMTCTL(*PMTRQS) +
                          PROMPT('Apply requisite PTFs')
