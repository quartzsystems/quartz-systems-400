/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDLNK: a symbolic or hard link to an object. */
             CMD        PROMPT('Add Link') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(ADDLNK)
             PARM       KWD(OBJ) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Object')
             PARM       KWD(NEWLNK) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('New link')
             PARM       KWD(LNKTYPE) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*SYMBOLIC) VALUES(*SYMBOLIC *HARD) +
                          PROMPT('Link type')
