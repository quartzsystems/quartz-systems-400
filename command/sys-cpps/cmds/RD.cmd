/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RD: remove a directory (RMDIR's alias). */
             CMD        PROMPT('Remove Directory') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(RD)
             PARM       KWD(DIR) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Directory')
             PARM       KWD(SUBTREE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL) +
                          PROMPT('Remove subtree')
             PARM       KWD(RMVLNK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Remove links in directory')
