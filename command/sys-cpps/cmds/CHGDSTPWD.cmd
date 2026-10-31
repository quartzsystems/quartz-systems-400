/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGDSTPWD: put service tools user ID QSECOFR back to its shipped,  */
/* expired password and enable it; only QSECOFR may run it.            */
             CMD        PROMPT('Change IBM Service Tools Pwd') MAXPOS(1) +
                          ALLOW(*INTERACT *IPGM *IMOD *EXEC) +
                          HLPPNLGRP(QHSYCMD) HLPID(CHGDSTPWD)
             PARM       KWD(PASSWORD) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *DEFAULT) +
                          PROMPT('Password')
