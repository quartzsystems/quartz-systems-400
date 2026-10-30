/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGATR: change one attribute of IFS objects. */
             CMD        PROMPT('Change Attribute') MAXPOS(3) +
                          ALLOW(*ALL) HLPPNLGRP(QHP0LCMD) HLPID(CHGATR)
             PARM       KWD(OBJ) TYPE(*PNAME) LEN(5000) MIN(1) +
                          PROMPT('Object')
             PARM       KWD(ATR) TYPE(*INT4) RSTD(*YES) MIN(1) +
                          SPCVAL((*READONLY 1) (*HIDDEN 2) (*PCSYSTEM 3) +
                          (*PCARCHIVE 4) (*SYSARCHIVE 5) (*CCSID 6) +
                          (*ALWCKPWRT 7) (*USECOUNT 8) (*DISKSTGOPT 9) +
                          (*MAINSTGOPT 10) (*CRTOBJSCAN 11) (*SCAN 12) +
                          (*ALWSAV 13) (*RSTDRNMUNL 14) (*SETUID 15) +
                          (*SETGID 16) (*CRTOBJAUD 17) (*INHCKPWRT 18) +
                          (*TEXT 19) (*RUNEXIT 20) (*CRTRUNEXIT 21)) +
                          PROMPT('Attribute')
             PARM       KWD(VALUE) TYPE(*INT4) MIN(1) RANGE(1 65533) +
                          SPCVAL((*YES -1) (*NO -2) (*RESET -3) +
                          (*NORMAL -4) (*MINIMIZE -5) (*DYNAMIC -6) +
                          (*CHGONLY -7) (*SYSVAL -8) (*NONE -9) +
                          (*USRPRF -10) (*ALL -11) (*CHANGE -12)) +
                          PROMPT('New value')
             PARM       KWD(SUBTREE) TYPE(*CHAR) LEN(5) RSTD(*YES) +
                          DFT(*NONE) VALUES(*NONE *ALL) +
                          PROMPT('Directory subtree')
             PARM       KWD(SYMLNK) TYPE(*CHAR) LEN(4) RSTD(*YES) +
                          DFT(*NO) VALUES(*NO *YES) +
                          PROMPT('Symbolic link')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) DFT(*BLANK) +
                          SPCVAL((*BLANK)) EXPR(*YES) +
                          PROMPT('Text description')
