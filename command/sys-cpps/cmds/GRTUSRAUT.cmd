/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* GRTUSRAUT: give a user every private authority another user holds. */
             CMD        PROMPT('Grant User Authority') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSYCMD) HLPID(GRTUSRAUT)
             PARM       KWD(USER) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('User')
             PARM       KWD(REFUSER) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Reference user')
