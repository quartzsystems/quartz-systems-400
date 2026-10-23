/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKOBJPDM: a library's objects with PDM's options. */
             CMD        PROMPT('Work with Objects Using PDM') +
                          MAXPOS(1) ALLOW(*INTERACT *IPGM *IREXX *EXEC +
                          *IMOD) HLPPNLGRP(QHPDMCMD) HLPID(WRKOBJPDM)
             PARM       KWD(LIB) TYPE(*NAME) LEN(10) DFT(*PRV) +
                          SPCVAL((*PRV) (*CURLIB)) PROMPT('Library')
             PARM       KWD(OBJ) TYPE(*GENERIC) LEN(10) DFT(*PRV) +
                          SPCVAL((*PRV) (*ALL)) PROMPT('Object')
             PARM       KWD(OBJTYPE) TYPE(*CHAR) LEN(10) DFT(*PRV) +
                          SPCVAL((*PRV) (*ALL)) PROMPT('Object type')
             PARM       KWD(OBJATR) TYPE(*GENERIC) LEN(10) DFT(*PRV) +
                          SPCVAL((*PRV) (*ALL) (*BLANK ' ')) +
                          PROMPT('Object attribute')
