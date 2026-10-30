/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTLIND: delete line descriptions that are varied off. */
             CMD        PROMPT('Delete Line Description') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDCLIN) HLPID(DLTLIND)
             PARM       KWD(LIND) TYPE(*GENERIC) LEN(10) MIN(1) +
                          PROMPT('Line description')
