/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKLIBPDM: the libraries with PDM's options. */
             CMD        PROMPT('Work with Libraries Using PDM') +
                          MAXPOS(1) ALLOW(*INTERACT *IPGM *IREXX *EXEC +
                          *IMOD) HLPPNLGRP(QHPDMCMD) HLPID(WRKLIBPDM)
             PARM       KWD(LIB) TYPE(*GENERIC) LEN(10) DFT(*PRV) +
                          SPCVAL((*PRV) (*LIBL) (*ALL) (*ALLUSR) +
                          (*USRLIBL) (*CURLIB)) PROMPT('Library')
