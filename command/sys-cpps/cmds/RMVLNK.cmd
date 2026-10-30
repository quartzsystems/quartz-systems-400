/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVLNK: remove a link; a pattern in the last name removes each match. */
             CMD        PROMPT('Remove Link') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(RMVLNK)
             PARM       KWD(OBJLNK) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Object link')
