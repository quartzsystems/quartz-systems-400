/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* WRKMBRPDM: a file's members with PDM's options. */
             CMD        PROMPT('Work with Members Using PDM') +
                          MAXPOS(1) ALLOW(*INTERACT *IPGM *IREXX *EXEC +
                          *IMOD) HLPPNLGRP(QHPDMCMD) HLPID(WRKMBRPDM)
             PARM       KWD(FILE) TYPE(Q1) DFT(*PRV) SNGVAL((*PRV)) +
                          PROMPT('File')
             PARM       KWD(MBR) TYPE(*GENERIC) LEN(10) DFT(*PRV) +
                          SPCVAL((*PRV) (*ALL)) PROMPT('Member')
             PARM       KWD(MBRTYPE) TYPE(*GENERIC) LEN(10) DFT(*PRV) +
                          SPCVAL((*PRV) (*ALL) (*BLANK ' ')) +
                          PROMPT('Member type')
             Q1:        QUAL       TYPE(*NAME) LEN(10)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*PRV) +
                          SPCVAL((*PRV) (*LIBL) (*CURLIB)) +
                          PROMPT('Library')
