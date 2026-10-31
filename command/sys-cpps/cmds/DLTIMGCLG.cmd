/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* DLTIMGCLG: delete an image catalog; KEEP(*NO) deletes its images. */
             CMD        PROMPT('Delete Image Catalog') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHOPCMD) +
                          HLPID(DLTIMGCLG)
             PARM       KWD(IMGCLG) TYPE(*GENERIC) LEN(10) MIN(1) +
                          PROMPT('Image catalog')
             PARM       KWD(KEEP) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*YES) VALUES(*YES *NO) +
                          PROMPT('Keep optical image files')
