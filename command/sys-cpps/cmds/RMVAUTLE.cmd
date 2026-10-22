/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVAUTLE: remove users from an authorization list. */
             CMD        PROMPT('Remove Auth List Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(RMVAUTLE)
             PARM       KWD(AUTL) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Authorization list')
             PARM       KWD(USER) TYPE(*NAME) LEN(10) MIN(1) MAX(50) +
                          SNGVAL((*PUBLIC) (*ALL)) PROMPT('Users')
