/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* STRTCP: start TCP/IP: the interfaces, routes and servers that start with it. */
             CMD        PROMPT('Start TCP/IP') ALLOW(*ALL) +
                          HLPPNLGRP(QHTOCCMD) HLPID(STRTCP)
             PARM       KWD(STRSVR) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*YES) +
                          VALUES(*YES *NO) PROMPT('Start the servers')
             PARM       KWD(STRIFC) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*YES) +
                          VALUES(*YES *NO) PROMPT('Start the interfaces')
             PARM       KWD(STRPTPPRF) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*YES) +
                          VALUES(*YES *NO) PROMPT('Start point-to-point profiles')
             PARM       KWD(STRIP6) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*YES) +
                          VALUES(*YES *NO) PROMPT('Start IPv6')
