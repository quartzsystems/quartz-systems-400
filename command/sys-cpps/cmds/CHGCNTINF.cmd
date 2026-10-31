/* Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is */
/* licensed under the terms of the MIT license which can be found in the  */
/* root directory of this project.                                       */

/* CHGCNTINF: change the contact information sent with orders and */
/* service requests.                                             */
             CMD        PROMPT('Change Contact Information') +
                          ALLOW(*ALL) HLPPNLGRP(QHSVCMD) HLPID(CHGCNTINF)
             PARM       KWD(CMPNY) TYPE(*CHAR) LEN(36) DFT(*SAME) +
                          SPCVAL((*SAME)) CASE(*MIXED) +
                          PROMPT('Company')
             PARM       KWD(CONTACT) TYPE(*CHAR) LEN(36) DFT(*SAME) +
                          SPCVAL((*SAME)) CASE(*MIXED) +
                          PROMPT('Contact')
             PARM       KWD(TELNBR) TYPE(*CHAR) LEN(30) DFT(*SAME) +
                          SPCVAL((*SAME)) PROMPT('Telephone number')
             PARM       KWD(EMAILADR) TYPE(*CHAR) LEN(128) DFT(*SAME) +
                          SPCVAL((*SAME)) CASE(*MIXED) +
                          PROMPT('E-mail address')
             PARM       KWD(MAILADDR) TYPE(E1) DFT(*SAME) +
                          SNGVAL((*SAME)) PROMPT('Mailing address')
             PARM       KWD(MEDPTF) TYPE(*CHAR) LEN(10) RSTD(*YES) +
                          DFT(*SAME) VALUES(*SAME *AUTOMATIC *DVDRAM +
                          *DVDROM) PROMPT('Media for mailing PTFs')
             E1:        ELEM       TYPE(*CHAR) LEN(36) CASE(*MIXED) +
                          PROMPT('Street address')
                        ELEM       TYPE(*CHAR) LEN(36) CASE(*MIXED) +
                          PROMPT('City or locality')
                        ELEM       TYPE(*CHAR) LEN(20) CASE(*MIXED) +
                          PROMPT('State or province')
                        ELEM       TYPE(*CHAR) LEN(20) CASE(*MIXED) +
                          PROMPT('Country or region')
                        ELEM       TYPE(*CHAR) LEN(12) CASE(*MIXED) +
                          PROMPT('Postal code')
