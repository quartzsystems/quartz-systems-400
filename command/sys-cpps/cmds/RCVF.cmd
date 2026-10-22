/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RCVF: read a record from the declared file into its field variables. */
             CMD        PROMPT('Receive File') MAXPOS(2) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(RCVF)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) DFT(*FILE) +
                          SPCVAL((*FILE)) PROMPT('Device')
             PARM       KWD(RCDFMT) TYPE(*NAME) LEN(10) DFT(*FILE) +
                          SPCVAL((*FILE)) PROMPT('Record format')
             PARM       KWD(WAIT) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*YES) +
                          VALUES(*YES *NO) PROMPT('Wait for record')
             PARM       KWD(OPNID) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Open file identifier')
