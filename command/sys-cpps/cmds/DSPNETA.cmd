/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DSPNETA: show the network attributes. */
             CMD        PROMPT('Display Network Attributes') MAXPOS(0) +
                          ALLOW(*ALL) HLPPNLGRP(QHCFCMD) HLPID(DSPNETA)
             PARM       KWD(OUTPUT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*) VALUES(* *PRINT) PROMPT('Output')
