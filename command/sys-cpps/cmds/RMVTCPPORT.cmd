/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVTCPPORT: remove port restrictions. */
             CMD        PROMPT('Remove TCP/IP Port Restriction') +
                          MAXPOS(3) ALLOW(*ALL) HLPPNLGRP(QHTOCCMD) +
                          HLPID(RMVTCPPORT)
             PARM       KWD(PORT) TYPE(E1) MIN(1) PROMPT('Port')
             PARM       KWD(PROTOCOL) TYPE(*NAME) LEN(4) RSTD(*YES) +
                          SPCVAL((*UDP) (*TCP)) MIN(1) +
                          PROMPT('Protocol')
             PARM       KWD(USRPRF) TYPE(*CHAR) LEN(10) MIN(1) +
                          PROMPT('User profile allowed')
             E1:        ELEM       TYPE(*INT4) RANGE(1 65535) +
                          SPCVAL((*ALL -1)) MIN(1) +
                          PROMPT('Lowest port')
                        ELEM       TYPE(*INT4) DFT(*ONLY) RANGE(1 +
                          65535) SPCVAL((*ONLY -1)) +
                          PROMPT('Highest port')
