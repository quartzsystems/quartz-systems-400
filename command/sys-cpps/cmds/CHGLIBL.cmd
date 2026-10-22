/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGLIBL: replace the user part of the library list, and the */
/* current library if asked.                                     */
             CMD        PROMPT('Change Library List') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(CHGLIBL)
             PARM       KWD(LIBL) TYPE(*NAME) LEN(10) MIN(1) MAX(250) +
                          SNGVAL((*NONE)) PROMPT('Libraries for current thread')
             PARM       KWD(CURLIB) TYPE(*NAME) LEN(10) DFT(*SAME) +
                          SPCVAL((*SAME) (*CRTDFT)) PROMPT('Current library')
