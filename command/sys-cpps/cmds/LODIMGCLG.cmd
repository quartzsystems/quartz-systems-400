/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* LODIMGCLG: load an image catalog on a virtual device, or unload it. */
             CMD        PROMPT('Load or Unload Image Catalog') +
                          MAXPOS(1) ALLOW(*ALL) HLPPNLGRP(QHOPCMD) +
                          HLPID(LODIMGCLG)
             PARM       KWD(IMGCLG) TYPE(*NAME) LEN(10) MIN(1) +
                          KEYPARM(*YES) PROMPT('Image catalog')
             PARM       KWD(OPTION) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*LOAD) VALUES(*LOAD *UNLOAD *MOUNT) +
                          KEYPARM(*YES) PROMPT('Option')
             PARM       KWD(DEV) TYPE(*NAME) LEN(10) +
                          PROMPT('Device')
