/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* PWRDWNSYS: end every subsystem and power the system down, with an */
/* IPL afterwards if asked.                                            */
             CMD        PROMPT('Power Down System') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHWMCMD) HLPID(PWRDWNSYS)
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*CNTRLD) VALUES(*CNTRLD *IMMED) +
                          PROMPT('How to end')
             PARM       KWD(DELAY) TYPE(*INT4) DFT(3600) RANGE(1 99999) +
                          SPCVAL((*NOLIMIT 0)) +
                          PROMPT('Controlled end delay time')
             PARM       KWD(RESTART) TYPE(E1) DFT(*NO) PROMPT('Restart')
             PARM       KWD(IPLSRC) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*PANEL) VALUES(*PANEL A B D *IMGCLG) +
                          PROMPT('IPL source')
             PARM       KWD(IMGCLG) TYPE(*NAME) LEN(10) PMTCTL(P1) +
                          PROMPT('Image catalog')
             PARM       KWD(ENDSBSOPT) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          MAX(3) DFT(*DFT) VALUES(*NOJOBLOG *CHGPTY +
                          *CHGTSL) SNGVAL((*DFT)) +
                          PROMPT('End subsystem option')
             PARM       KWD(TIMOUTOPT) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*CONTINUE) VALUES(*CONTINUE *POWEROFF) +
                          PROMPT('Timeout option')
             PARM       KWD(CONFIRM) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) PROMPT('Confirm')
             P1:        PMTCTL     CTL(IPLSRC) COND((*EQ *IMGCLG))
             E1:        ELEM       TYPE(*CHAR) LEN(4) RSTD(*YES) DFT(*NO) +
                          VALUES(*NO *YES) PROMPT('Restart after power down')
                        ELEM       TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*IPLA) VALUES(*IPLA *SYS *FULL) +
                          PROMPT('Restart type')
