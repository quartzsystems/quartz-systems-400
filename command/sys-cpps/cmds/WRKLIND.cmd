/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKLIND: list line descriptions and work with them. */
             CMD        PROMPT('Work with Line Descriptions') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDCLIN) HLPID(WRKLIND)
             PARM       KWD(LIND) TYPE(*GENERIC) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL) (*ASYNC) (*BSC) (*DDI) (*ELAN) +
                          (*FAX) (*FR) (*IDLC) (*NET) (*PPP) (*SDLC) +
                          (*TDLC) (*TRLAN) (*WLS) (*X25)) +
                          PROMPT('Line description')
