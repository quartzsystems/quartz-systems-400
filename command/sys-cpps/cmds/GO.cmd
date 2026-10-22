/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* GO: show a menu. */
             CMD        PROMPT('Go to Menu') MAXPOS(1) +
                          ALLOW(*INTERACT *IPGM *IMOD *EXEC) +
                          HLPPNLGRP(QHSVCMD) HLPID(GO)
             PARM       KWD(MENU) TYPE(Q1) MIN(1) PROMPT('Menu')
             PARM       KWD(RTNPNT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Return point')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALL) +
                          (*ALLUSR)) PROMPT('Library')
