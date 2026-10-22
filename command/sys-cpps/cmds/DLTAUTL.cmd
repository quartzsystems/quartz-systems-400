/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTAUTL: delete an authorization list. */
             CMD        PROMPT('Delete Authorization List') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(DLTAUTL)
             PARM       KWD(AUTL) TYPE(*GENERIC) LEN(10) MIN(1) +
                          PROMPT('Authorization list')
