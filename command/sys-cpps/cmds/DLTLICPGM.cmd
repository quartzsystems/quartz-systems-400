/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTLICPGM: delete a licensed program or one of its options. */
             CMD        PROMPT('Delete Licensed Program') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHSZCMD) HLPID(DLTLICPGM)
             PARM       KWD(LICPGM) TYPE(*CHAR) LEN(7) MIN(1) +
                          PROMPT('Product')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(5) DFT(*ALL) +
                          SPCVAL((*ALL) (*BASE)) +
                          PROMPT('Optional part to be deleted')
             PARM       KWD(RLS) TYPE(*CHAR) LEN(6) DFT(*ONLY) +
                          SPCVAL((*ONLY)) PROMPT('Release')
             PARM       KWD(LNG) TYPE(*CHAR) LEN(8) DFT(*ALL) +
                          SPCVAL((*ALL)) PROMPT('Language for licensed program')
