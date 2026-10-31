/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* LODIMGCLGE: load, unload or mount one image catalog entry.        */
             CMD        PROMPT('Load/Unload Image Catalog Entry') +
                          MAXPOS(3) ALLOW(*ALL) HLPPNLGRP(QHOPCMD) +
                          HLPID(LODIMGCLGE)
             PARM       KWD(IMGCLG) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Image catalog')
             PARM       KWD(IMGCLGIDX) TYPE(*INT2) MIN(1) +
                          RANGE(1 256) PROMPT('Image catalog index')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*LOAD) VALUES(*LOAD *UNLOAD *MOUNT) +
                          PROMPT('Option')
