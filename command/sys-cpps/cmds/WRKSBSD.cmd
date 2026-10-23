/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKSBSD: list subsystem descriptions and work with them. */
             CMD        PROMPT('Work with Subsystem Descriptions') +
                          MAXPOS(1) ALLOW(*INTERACT *IPGM *IREXX *EXEC +
                          *IMOD) HLPPNLGRP(QHWMCMD) HLPID(WRKSBSD)
             PARM       KWD(SBSD) TYPE(Q1) DFT(*ALL) +
                          PROMPT('Subsystem description')
             Q1:        QUAL       TYPE(*GENERIC) LEN(10) DFT(*ALL) +
                          SPCVAL((*ALL))
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB) (*USRLIBL) (*ALLUSR) +
                          (*ALL)) PROMPT('Library')
