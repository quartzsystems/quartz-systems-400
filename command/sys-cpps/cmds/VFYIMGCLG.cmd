/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* VFYIMGCLG: verify that an image catalog's images are there and,   */
/* for the install types, form one ordered set.                      */
             CMD        PROMPT('Verify Image Catalog') MAXPOS(2) +
                          ALLOW(*ALL) HLPPNLGRP(QHOPCMD) +
                          HLPID(VFYIMGCLG)
             PARM       KWD(IMGCLG) TYPE(*NAME) LEN(10) MIN(1) +
                          PROMPT('Image catalog')
             PARM       KWD(TYPE) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*UPGRADE) VALUES(*UPGRADE *LIC *PTF +
                          *OTHER) PROMPT('Verify type')
             PARM       KWD(SORT) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*YES *NO) +
                          PROMPT('Sort image catalog')
             PARM       KWD(NFSSHR) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*YES *NO) +
                          PROMPT('Network file server share')
             PARM       KWD(LNG) TYPE(*CHAR) LEN(4) DFT(*DFT) +
                          SPCVAL((*DFT)) PROMPT('Language')
