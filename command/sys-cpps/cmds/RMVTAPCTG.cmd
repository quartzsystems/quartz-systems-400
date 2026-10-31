/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVTAPCTG: eject cartridges from a media library.                  */
             CMD        PROMPT('Remove Tape Cartridge') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) +
                          HLPID(RMVTAPCTG)
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Library device')
             PARM       KWD(CTG) TYPE(*CHAR) LEN(6) MIN(1) MAX(40) +
                          PROMPT('Cartridge ID')
             PARM       KWD(STATION) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*DEV) VALUES(*DEV *CNV *BULK) +
                          PROMPT('Station')
