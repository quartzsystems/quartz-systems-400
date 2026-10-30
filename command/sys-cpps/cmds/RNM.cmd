/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RNM: give an object link a new name in its directory. */
             CMD        PROMPT('Rename Object') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(RNM)
             PARM       KWD(OBJ) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Object')
             PARM       KWD(NEWOBJ) TYPE(*CHAR) LEN(1024) MIN(1) +
                          PROMPT('New object name')
