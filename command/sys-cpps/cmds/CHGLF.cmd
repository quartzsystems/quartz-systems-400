/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGLF: change a logical file's attributes. */
             CMD        PROMPT('Change Logical File') MAXPOS(1) +
                          ALLOW(*ALL) HLPPNLGRP(QHDBCMD) HLPID(CHGLF)
             PARM       KWD(FILE) TYPE(Q1) MIN(1) PROMPT('File')
             PARM       KWD(SYSTEM) TYPE(*CHAR) LEN(9) RSTD(*YES) +
                          DFT(*LCL) VALUES(*LCL *RMT *FILETYPE) +
                          PROMPT('System')
             PARM       KWD(MAXMBRS) TYPE(*DEC) LEN(5) DFT(*SAME) +
                          RANGE(1 32767) SPCVAL((*SAME -1) (*NOMAX 0)) +
                          PROMPT('Maximum members')
             PARM       KWD(ACCPTHSIZ) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *MAX1TB *MAX4GB) +
                          PROMPT('Access path size')
             PARM       KWD(PAGESIZE) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *KEYLEN 8 16 32 64 +
                          128 256 512) +
                          PROMPT('Access path logical page size')
             PARM       KWD(MAINT) TYPE(*CHAR) LEN(6) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *IMMED *REBLD *DLY) +
                          PROMPT('Access path maintenance')
             PARM       KWD(RECOVER) TYPE(*CHAR) LEN(7) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *AFTIPL *IPL) +
                          PROMPT('Access path recovery')
             PARM       KWD(FRCACCPTH) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Force keyed access path')
             PARM       KWD(UNIT) TYPE(*DEC) LEN(3) DFT(*SAME) +
                          RANGE(1 255) SPCVAL((*SAME -1) (*ANY 0)) +
                          PROMPT('Preferred storage unit')
             PARM       KWD(FRCRATIO) TYPE(*DEC) LEN(5) DFT(*SAME) +
                          RANGE(1 32767) SPCVAL((*SAME -1) (*NONE 0)) +
                          PROMPT('Records to force a write')
             PARM       KWD(WAITFILE) TYPE(*DEC) LEN(5) DFT(*SAME) +
                          RANGE(1 32767) SPCVAL((*SAME -2) (*IMMED 0) +
                          (*CLS -1)) PROMPT('Maximum file wait time')
             PARM       KWD(WAITRCD) TYPE(*DEC) LEN(5) DFT(*SAME) +
                          RANGE(1 32767) SPCVAL((*SAME -2) (*IMMED 0) +
                          (*NOMAX -1)) PROMPT('Maximum record wait time')
             PARM       KWD(SHARE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *NO *YES) +
                          PROMPT('Share open data path')
             PARM       KWD(LVLCHK) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *YES *NO) +
                          PROMPT('Record format level check')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*SAME) +
                          SPCVAL((*SAME) (*BLANK ' ')) +
                          PROMPT('Description')
             Q1:        QUAL       TYPE(*NAME) LEN(10) MIN(1)
                        QUAL       TYPE(*NAME) LEN(10) DFT(*LIBL) +
                          SPCVAL((*LIBL) (*CURLIB)) PROMPT('Library')
