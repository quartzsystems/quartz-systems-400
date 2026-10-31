/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVLICKEY: remove license keys. */
             CMD        PROMPT('Remove License Key Information') +
                          MAXPOS(3) ALLOW(*ALL) HLPPNLGRP(QHLZCMD) +
                          HLPID(RMVLICKEY)
             PARM       KWD(PRDID) TYPE(*CHAR) LEN(7) MIN(1) +
                          PROMPT('Product identifier')
             PARM       KWD(LICTRM) TYPE(*CHAR) LEN(6) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('License term')
             PARM       KWD(FEATURE) TYPE(*CHAR) LEN(4) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Feature')
             PARM       KWD(SERIAL) TYPE(*CHAR) LEN(8) DFT(*LOCAL) +
                          SPCVAL((*LOCAL) (*ALL)) +
                          PROMPT('System serial number')
             PARM       KWD(PRCGRP) TYPE(*CHAR) LEN(4) DFT(*ALL) +
                          SPCVAL((*ALL) (*ANY)) PROMPT('Processor group')
