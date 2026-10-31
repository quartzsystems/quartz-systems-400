/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* QSPKGPRD: build a product from a manifest stream file (Quartz's). */
             CMD        PROMPT('Package Product from Manifest') +
                          MAXPOS(2) ALLOW(*ALL) HLPPNLGRP(QHSZCMD) +
                          HLPID(QSPKGPRD)
             PARM       KWD(MANIFEST) TYPE(*PNAME) LEN(256) MIN(1) +
                          PROMPT('Manifest stream file')
             PARM       KWD(SAVF) TYPE(Q1) DFT(*NONE) SNGVAL((*NONE)) +
                          PROMPT('Save file for the product')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
