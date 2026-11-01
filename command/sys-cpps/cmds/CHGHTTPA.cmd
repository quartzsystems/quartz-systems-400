/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGHTTPA: the HTTP server's attributes, kept in the TCP/IP server   */
/* table in QUSRSYS. AUTOSTART says whether STRTCP starts *HTTP (the    */
/* administration instance). IBM's NBRSVR, CCSID and translation-table  */
/* keywords set up the original server, which reads none of them here. */
             CMD        PROMPT('Change HTTP Attributes') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHTMCMD) HLPID(CHGHTTPA)
             PARM       KWD(AUTOSTART) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*YES *NO *SAME) +
                          PROMPT('Autostart')
