/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDOPTCTG: add an optical cartridge to an optical media library.   */
             CMD        PROMPT('Add Optical Cartridge') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHOPCMD) +
                          HLPID(ADDOPTCTG)
             PARM       KWD(MLB) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Optical device')
             PARM       KWD(AUTL) TYPE(*NAME) LEN(10) DFT(*PRV) +
                          SPCVAL((*PRV) (QOPTSEC) (*NONE)) +
                          PROMPT('Authorization list')
             PARM       KWD(DIR) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Rebuild directory index')
             PARM       KWD(MEDLOC) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*IOSTATION) VALUES(*IOSTATION *MAGAZINE) +
                          PROMPT('Media location')
