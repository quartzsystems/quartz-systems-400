       // Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
       // licensed under the terms of the MIT license which can be found in the
       // root directory of this project.
       //
       // QOAR/QRPGLESRC,QRNTYPES: the named constants that Open Access
       // handlers, DATA-INTO parsers and DATA-GEN generators share.
       // Its lines are numbered as IBM's member's are, so listings count alike.
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
       //
      /IF DEFINED(QrnTypes_Included)
      /EOF
      /ENDIF
      /DEFINE QrnTypes_Included

       // How the second operand of %PARSER, %GEN or HANDLER was passed.
       DCL-C QrnUserParmType_notPassed '0';
       DCL-C QrnUserParmType_nullTerminatedString '1';
       DCL-C QrnUserParmType_indicator '2';
       DCL-C QrnUserParmType_char '3';
       DCL-C QrnUserParmType_varchar_2 '4';
       DCL-C QrnUserParmType_varchar_4 '5';
       DCL-C QrnUserParmType_graph '6';
       DCL-C QrnUserParmType_vargraph_2 '7';
       DCL-C QrnUserParmType_vargraph_4 '8';
       DCL-C QrnUserParmType_ucs2 '9';
       DCL-C QrnUserParmType_varucs2_2 'a';
       DCL-C QrnUserParmType_varucs2_4 'b';
       DCL-C QrnUserParmType_dataStruct 'c';
       DCL-C QrnUserParmType_other 'd';

       // The type of a field or value.
       DCL-C QrnDatatype_Alpha 1;
       DCL-C QrnDatatype_AlphaVarying 2;
       DCL-C QrnDatatype_Ucs2 3;
       DCL-C QrnDatatype_Ucs2Varying 4;
       // UCS-2 and UTF-16 (not UTF-8).
       DCL-C QrnDatatype_Unicode 3;
       DCL-C QrnDatatype_UnicodeVarying 4;
       DCL-C QrnDatatype_Dbcs 5;
       DCL-C QrnDatatype_DbcsVarying 6;
       DCL-C QrnDatatype_Indicator 7;
       DCL-C QrnDatatype_Decimal 8;
       DCL-C QrnDatatype_Integer 9;
       DCL-C QrnDatatype_Unsigned 10;
       DCL-C QrnDatatype_Float 11;
       DCL-C QrnDatatype_Date 12;
       DCL-C QrnDatatype_Time 13;
       DCL-C QrnDatatype_Timestamp 14;

       // The format of a date, time or timestamp.
       DCL-C QrnDtzFormat_ISO 1;
       DCL-C QrnDtzFormat_USA 2;
       DCL-C QrnDtzFormat_EUR 3;
       DCL-C QrnDtzFormat_JIS 4;
       DCL-C QrnDtzFormat_YMD 5;
       DCL-C QrnDtzFormat_MDY 6;
       DCL-C QrnDtzFormat_DMY 7;
       DCL-C QrnDtzFormat_JUL 8;
       DCL-C QrnDtzFormat_HMS 9;






































