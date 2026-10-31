/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGIMGCLGE: change an image catalog entry's write protection or   */
/* text.                                                              */
             CMD        PROMPT('Change Image Catalog Entry') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHOPCMD) +
                          HLPID(CHGIMGCLGE)
             PARM       KWD(IMGCLG) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Image catalog')
             PARM       KWD(IMGCLGIDX) TYPE(*INT2) MIN(1) +
                          RANGE(1 256) PROMPT('Image catalog index')
             PARM       KWD(WRTPTC) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Write protect')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Text ''description''')
