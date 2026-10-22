/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ENDRCV: cancel a RCVF or SNDRCVF that was issued with WAIT(*NO). */
             CMD        PROMPT('End Receive') MAXPOS(2) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(ENDRCV)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) DFT(*FILE) +
                          SPCVAL((*FILE)) PROMPT('Device')
             PARM       KWD(RCDFMT) TYPE(*NAME) LEN(10) DFT(*FILE) +
                          SPCVAL((*FILE)) PROMPT('Record format')
             PARM       KWD(OPNID) TYPE(*NAME) LEN(10) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Open file identifier')
