/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* RTVUSRPRF: put the attributes of a user profile into CL variables. */
             CMD        PROMPT('Retrieve User Profile') MAXPOS(1) +
                          ALLOW(*BPGM *IPGM *BMOD *IMOD) +
                          HLPPNLGRP(QHCLCMD) HLPID(RTVUSRPRF)
             PARM       KWD(USRPRF) TYPE(*NAME) LEN(10) DFT(*CURRENT) +
                          SPCVAL((*CURRENT)) PROMPT('User profile')
             PARM       KWD(RTNUSRPRF) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for RTNUSRPRF (10)')
             PARM       KWD(SPCAUT) TYPE(*CHAR) LEN(100) RTNVAL(*YES) +
                          PROMPT('CL var for SPCAUT (100)')
             PARM       KWD(MAXSTG) TYPE(*DEC) LEN(11 0) RTNVAL(*YES) +
                          PROMPT('CL var for MAXSTG (11 0)')
             PARM       KWD(MAXSTGLRG) TYPE(*DEC) LEN(15 0) RTNVAL(*YES) +
                          PROMPT('CL var for MAXSTGLRG (15 0)')
             PARM       KWD(STGUSED) TYPE(*DEC) LEN(11 0) RTNVAL(*YES) +
                          PROMPT('CL var for STGUSED (11 0)')
             PARM       KWD(STGUSEDLRG) TYPE(*DEC) LEN(15 0) RTNVAL(*YES) +
                          PROMPT('CL var for STGUSEDLRG (15 0)')
             PARM       KWD(PTYLMT) TYPE(*CHAR) LEN(1) RTNVAL(*YES) +
                          PROMPT('CL var for PTYLMT (1)')
             PARM       KWD(INLPGM) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for INLPGM (10)')
             PARM       KWD(INLPGMLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for INLPGMLIB (10)')
             PARM       KWD(JOBD) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for JOBD (10)')
             PARM       KWD(JOBDLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for JOBDLIB (10)')
             PARM       KWD(GRPPRF) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for GRPPRF (10)')
             PARM       KWD(OWNER) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for OWNER (10)')
             PARM       KWD(GRPAUT) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for GRPAUT (10)')
             PARM       KWD(GRPAUTTYP) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for GRPAUTTYP (10)')
             PARM       KWD(ACGCDE) TYPE(*CHAR) LEN(15) RTNVAL(*YES) +
                          PROMPT('CL var for ACGCDE (15)')
             PARM       KWD(MSGQ) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for MSGQ (10)')
             PARM       KWD(MSGQLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for MSGQLIB (10)')
             PARM       KWD(DLVRY) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for DLVRY (10)')
             PARM       KWD(SEV) TYPE(*DEC) LEN(2 0) RTNVAL(*YES) +
                          PROMPT('CL var for SEV (2 0)')
             PARM       KWD(OUTQ) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for OUTQ (10)')
             PARM       KWD(OUTQLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for OUTQLIB (10)')
             PARM       KWD(PRTDEV) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for PRTDEV (10)')
             PARM       KWD(TEXT) TYPE(*CHAR) LEN(50) RTNVAL(*YES) +
                          PROMPT('CL var for TEXT (50)')
             PARM       KWD(SPCENV) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SPCENV (10)')
             PARM       KWD(DSPSGNINF) TYPE(*CHAR) LEN(7) RTNVAL(*YES) +
                          PROMPT('CL var for DSPSGNINF (7)')
             PARM       KWD(PWDEXPITV) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('CL var for PWDEXPITV (5 0)')
             PARM       KWD(PWDEXP) TYPE(*CHAR) LEN(4) RTNVAL(*YES) +
                          PROMPT('CL var for PWDEXP (4)')
             PARM       KWD(USRCLS) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for USRCLS (10)')
             PARM       KWD(STATUS) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for STATUS (10)')
             PARM       KWD(PWDCHGDAT) TYPE(*CHAR) LEN(13) RTNVAL(*YES) +
                          PROMPT('CL var for PWDCHGDAT (13)')
             PARM       KWD(PRVSIGN) TYPE(*CHAR) LEN(13) RTNVAL(*YES) +
                          PROMPT('CL var for PRVSIGN (13)')
             PARM       KWD(NOTVLDSIGN) TYPE(*DEC) LEN(11 0) RTNVAL(*YES) +
                          PROMPT('CL var for NOTVLDSIGN (11 0)')
             PARM       KWD(USROPT) TYPE(*CHAR) LEN(240) RTNVAL(*YES) +
                          PROMPT('CL var for USROPT (240)')
             PARM       KWD(CURLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for CURLIB (10)')
             PARM       KWD(INLMNU) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for INLMNU (10)')
             PARM       KWD(INLMNULIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for INLMNULIB (10)')
             PARM       KWD(LMTCPB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for LMTCPB (10)')
             PARM       KWD(LMTDEVSSN) TYPE(*CHAR) LEN(7) RTNVAL(*YES) +
                          PROMPT('CL var for LMTDEVSSN (7)')
             PARM       KWD(KBDBUF) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for KBDBUF (10)')
             PARM       KWD(ASTLVL) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for ASTLVL (10)')
             PARM       KWD(ATNPGM) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for ATNPGM (10)')
             PARM       KWD(ATNPGMLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for ATNPGMLIB (10)')
             PARM       KWD(SRTSEQ) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SRTSEQ (10)')
             PARM       KWD(SRTSEQLIB) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for SRTSEQLIB (10)')
             PARM       KWD(LANGID) TYPE(*CHAR) LEN(3) RTNVAL(*YES) +
                          PROMPT('CL var for LANGID (3)')
             PARM       KWD(CNTRYID) TYPE(*CHAR) LEN(2) RTNVAL(*YES) +
                          PROMPT('CL var for CNTRYID (2)')
             PARM       KWD(CCSID) TYPE(*DEC) LEN(5 0) RTNVAL(*YES) +
                          PROMPT('CL var for CCSID (5 0)')
             PARM       KWD(CHRIDCTL) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for CHRIDCTL (10)')
             PARM       KWD(SETJOBATR) TYPE(*CHAR) LEN(150) RTNVAL(*YES) +
                          PROMPT('CL var for SETJOBATR (150)')
             PARM       KWD(LOCALE) TYPE(*CHAR) LEN(2048) RTNVAL(*YES) +
                          PROMPT('CL var for LOCALE (2048)')
             PARM       KWD(SUPGRPPRF) TYPE(*CHAR) LEN(150) RTNVAL(*YES) +
                          PROMPT('CL var for SUPGRPPRF (150)')
             PARM       KWD(OBJAUD) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for OBJAUD (10)')
             PARM       KWD(AUDLVL) TYPE(*CHAR) LEN(640) RTNVAL(*YES) +
                          PROMPT('CL var for AUDLVL (640)')
             PARM       KWD(UID) TYPE(*DEC) LEN(10 0) RTNVAL(*YES) +
                          PROMPT('CL var for UID (10 0)')
             PARM       KWD(GID) TYPE(*DEC) LEN(10 0) RTNVAL(*YES) +
                          PROMPT('CL var for GID (10 0)')
             PARM       KWD(HOMEDIR) TYPE(*CHAR) LEN(2048) RTNVAL(*YES) +
                          PROMPT('CL var for HOMEDIR (2048)')
             PARM       KWD(LCLPWDMGT) TYPE(*CHAR) LEN(4) RTNVAL(*YES) +
                          PROMPT('CL var for LCLPWDMGT (4)')
             PARM       KWD(PWDCHGBLK) TYPE(*CHAR) LEN(10) RTNVAL(*YES) +
                          PROMPT('CL var for PWDCHGBLK (10)')
             PARM       KWD(USREXPDATE) TYPE(*CHAR) LEN(7) RTNVAL(*YES) +
                          PROMPT('CL var for USREXPDATE (7)')
             PARM       KWD(USREXPITV) TYPE(*DEC) LEN(3 0) RTNVAL(*YES) +
                          PROMPT('CL var for USREXPITV (3 0)')
