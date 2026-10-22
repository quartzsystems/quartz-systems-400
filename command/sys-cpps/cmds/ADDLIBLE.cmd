/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDLIBLE: add a library to the user part of the library list. */
             CMD        PROMPT('Add Library List Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(ADDLIBLE)
             PARM       KWD(LIB) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Library')
             PARM       KWD(POSITION) TYPE(E1) DFT(*FIRST) +
                          PROMPT('Library list position')
             E1:        ELEM       TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*FIRST) VALUES(*FIRST *LAST *AFTER *BEFORE +
                          *REPLACE) PROMPT('List position')
                        ELEM       TYPE(*NAME) LEN(10) +
                          PROMPT('Reference library')
