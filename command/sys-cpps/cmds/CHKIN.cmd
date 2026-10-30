/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHKIN: check an IFS object back in. */
             CMD        PROMPT('Check In Object') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(CHKIN)
             PARM       KWD(OBJ) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Object')
             PARM       KWD(SUBTREE) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL) +
                          PROMPT('Directory subtree')
