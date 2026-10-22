/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CRTAUTL: create an authorization list. */
             CMD        PROMPT('Create Authorization List') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(CRTAUTL)
             PARM       KWD(AUTL) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Authorization list')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PROMPT('Description')
             PARM       KWD(AUT) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*EXCLUDE) VALUES(*EXCLUDE *ALL *CHANGE *USE) +
                          PROMPT('Public authority')
