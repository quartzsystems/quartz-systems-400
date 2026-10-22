/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CLOSE: close a declared file so that the next RCVF reopens it. */
             CMD        PROMPT('Close File') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(CLOSE)
             PARM       KWD(OPNID) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Open file identifier')
