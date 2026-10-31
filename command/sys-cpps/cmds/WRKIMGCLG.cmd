/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKIMGCLG: work with image catalogs.                               */
             CMD        PROMPT('Work with Image Catalogs') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHOPCMD) +
                          HLPID(WRKIMGCLG)
             PARM       KWD(IMGCLG) TYPE(*GENERIC) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Image catalog')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*ALL) VALUES(*ALL *OPT *TAP) +
                          PROMPT('Image catalog type')
