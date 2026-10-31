/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* ADDIMGCLGE: add an image to an image catalog: a file in its       */
/* directory, a file copied in, or FROMFILE(*NEW) for a new volume.  */
             CMD        PROMPT('Add Image Catalog Entry') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHOPCMD) +
                          HLPID(ADDIMGCLGE)
             PARM       KWD(IMGCLG) TYPE(*NAME) LEN(10) MIN(1) +
                          KEYPARM(*YES) PROMPT('Image catalog')
             PARM       KWD(FROMDEV) TYPE(*NAME) LEN(10) +
                          KEYPARM(*YES) PROMPT('From optical device, or')
             PARM       KWD(FROMFILE) TYPE(*PNAME) LEN(1024) +
                          SPCVAL((*NEW)) CASE(*MIXED) +
                          PROMPT('From image file')
             PARM       KWD(TOFILE) TYPE(*PNAME) LEN(1024) DFT(*GEN) +
                          SPCVAL((*FROMFILE) (*GEN)) CASE(*MIXED) +
                          PROMPT('To image file')
             PARM       KWD(IMGCLGIDX) TYPE(*INT2) DFT(*AVAIL) +
                          RANGE(1 256) SPCVAL((*AVAIL 0)) +
                          PROMPT('Image catalog index')
             PARM       KWD(REPLACE) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*YES *NO) +
                          PROMPT('Replace catalog entry')
             PARM       KWD(IMGSIZ) TYPE(*INT4) DFT(*DVD4700) +
                          RANGE(1 16000000) SPCVAL((*CD650 650) +
                          (*DVD2600 2600) (*DVD4700 4700)) PMTCTL(PN) +
                          PROMPT('Image size')
             PARM       KWD(VOLNAM) TYPE(*CHAR) LEN(32) DFT(*GEN) +
                          SPCVAL((*GEN)) PMTCTL(PN) +
                          PROMPT('Volume name')
             PARM       KWD(ALCSTG) TYPE(*CHAR) LEN(8) RSTD(*YES) +
                          DFT(*MIN) VALUES(*MIN *IMGSIZ) PMTCTL(PN) +
                          PROMPT('Allocate storage size')
             PARM       KWD(WRTPTC) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*YES *NO) PMTCTL(PN) +
                          PROMPT('Write protect')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK ' ')) PMTCTL(PN) +
                          PROMPT('Text ''description''')
             PN:        PMTCTL     CTL(FROMFILE) COND((*EQ *NEW))
