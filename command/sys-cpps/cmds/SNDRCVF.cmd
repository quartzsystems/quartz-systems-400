/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* SNDRCVF: write a record format of the declared display file and */
/* read the reply.                                                     */
             CMD        PROMPT('Send/Receive File') MAXPOS(2) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(SNDRCVF)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) DFT(*FILE) +
                          SPCVAL((*FILE)) PROMPT('Device')
             PARM       KWD(RCDFMT) TYPE(*NAME) LEN(10) DFT(*FILE) +
                          SPCVAL((*FILE)) PROMPT('Record format')
             PARM       KWD(WAIT) TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*YES) +
                          VALUES(*YES *NO) PROMPT('Wait for record')
             PARM       KWD(OPNID) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Open file identifier')
