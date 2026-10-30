/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDTCP: end TCP/IP: its servers, then every interface. */
             CMD        PROMPT('End TCP/IP') ALLOW(*ALL) +
                          HLPPNLGRP(QHTOCCMD) HLPID(ENDTCP)
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(7) RSTD(*YES) DFT(*IMMED) +
                          VALUES(*IMMED *CNTRLD) PROMPT('How to end')
             PARM       KWD(DELAY) TYPE(*INT4) DFT(30) RANGE(1 86400) +
                          PROMPT('Delay for *CNTRLD (seconds)')
             PARM       KWD(ENDSVR) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*YES) +
                          VALUES(*YES *NO) PROMPT('End the servers')
