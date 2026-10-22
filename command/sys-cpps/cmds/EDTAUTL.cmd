/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* EDTAUTL: edit the entries of an authorization list on a screen. */
             CMD        PROMPT('Edit Authorization List') MAXPOS(1) +
                          ALLOW(*INTERACT *IPGM *IMOD *EXEC) +
                          HLPPNLGRP(QHSYCMD) HLPID(EDTAUTL)
             PARM       KWD(AUTL) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Authorization list')
