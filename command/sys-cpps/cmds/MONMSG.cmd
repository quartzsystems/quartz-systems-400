/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* MONMSG: monitor for escape, notify and status messages and run a */
/* command when one arrives.                                           */
             CMD        PROMPT('Monitor Message') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(MONMSG)
             PARM       KWD(MSGID) TYPE(*NAME) LEN(7) MIN(1) MAX(50) +
                          PROMPT('Message identifier')
             PARM       KWD(CMPDTA) TYPE(*CHAR) LEN(28) DFT(*NONE) +
                          SPCVAL((*NONE)) PROMPT('Comparison data')
             PARM       KWD(EXEC) TYPE(*CMDSTR) LEN(3000) +
                          PROMPT('Command to run')
