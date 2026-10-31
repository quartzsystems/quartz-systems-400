/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RMVIMGCLGE: remove an entry from an image catalog.                */
             CMD        PROMPT('Remove Image Catalog Entry') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHOPCMD) +
                          HLPID(RMVIMGCLGE)
             PARM       KWD(IMGCLG) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Image catalog')
             PARM       KWD(IMGCLGIDX) TYPE(*INT2) DFT(*VOL) +
                          RANGE(1 256) SPCVAL((*VOL 0)) +
                          PROMPT('Image catalog index')
             PARM       KWD(VOL) TYPE(*CHAR) LEN(32) +
                          PROMPT('Volume identifier')
             PARM       KWD(KEEP) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Keep image file')
