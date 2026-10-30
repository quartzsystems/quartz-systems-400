/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVTCPSVR: remove a user-defined server. */
             CMD        PROMPT('Remove TCP/IP Server') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) +
                          HLPID(RMVTCPSVR)
             PARM       KWD(SVRSPCVAL) TYPE(*CHAR) LEN(10) MIN(1) +
                          PROMPT('Server special value')
