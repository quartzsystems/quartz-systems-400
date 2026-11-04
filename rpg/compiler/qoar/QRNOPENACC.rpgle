       // Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
       // licensed under the terms of the MIT license which can be found in the
       // root directory of this project.
       //
       // QOAR/QRPGLESRC,QRNOPENACC: what an Open Access handler receives
       // for each operation on a HANDLER file.
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
       //
       //
       //
      /IF DEFINED(QrnOpenAccess_Included)
      /EOF
      /ENDIF
      /DEFINE QrnOpenAccess_Included

      /COPY QOAR/QRPGLESRC,QRNTYPES

       // The format of the parameter.
       DCL-C ROIO0100 'ROIO0100';

       // The file's device.
       DCL-C QrnDevice_Database 'D';
       DCL-C QrnDevice_Printer 'P';
       DCL-C QrnDevice_UserInterface 'U';

       // The operation, in rpgOperation.
       DCL-C QrnOperation_OPEN 1;
       DCL-C QrnOperation_POSITION_START 2;
       DCL-C QrnOperation_POSITION_END 3;
       DCL-C QrnOperation_READ 4;
       DCL-C QrnOperation_READC 5;
       DCL-C QrnOperation_READE 6;
       DCL-C QrnOperation_READP 7;
       DCL-C QrnOperation_READPE 8;
       DCL-C QrnOperation_CHAIN 9;
       DCL-C QrnOperation_EXFMT 10;
       DCL-C QrnOperation_SETGT 11;
       DCL-C QrnOperation_SETLL 12;
       DCL-C QrnOperation_UNLOCK 13;
       DCL-C QrnOperation_UPDATE 14;
       DCL-C QrnOperation_WRITE 15;
       DCL-C QrnOperation_DELETE 16;
       DCL-C QrnOperation_FEOD 17;
       DCL-C QrnOperation_CLOSE 18;
       DCL-C QrnOperation_DELETE_CURRENT 19;
       DCL-C QrnOperation_READE_CURRENT 20;
       DCL-C QrnOperation_READPE_CURRENT 21;

       // The key a workstation handler reports in functionKey.
       DCL-C QrnFunctionKey_None 0;
       DCL-C QrnFunctionKey_01 1;
       DCL-C QrnFunctionKey_02 2;
       DCL-C QrnFunctionKey_03 3;
       DCL-C QrnFunctionKey_04 4;
       DCL-C QrnFunctionKey_05 5;
       DCL-C QrnFunctionKey_06 6;
       DCL-C QrnFunctionKey_07 7;
       DCL-C QrnFunctionKey_08 8;
       DCL-C QrnFunctionKey_09 9;
       DCL-C QrnFunctionKey_10 10;
       DCL-C QrnFunctionKey_11 11;
       DCL-C QrnFunctionKey_12 12;
       DCL-C QrnFunctionKey_13 13;
       DCL-C QrnFunctionKey_14 14;
       DCL-C QrnFunctionKey_15 15;
       DCL-C QrnFunctionKey_16 16;
       DCL-C QrnFunctionKey_17 17;
       DCL-C QrnFunctionKey_18 18;
       DCL-C QrnFunctionKey_19 19;
       DCL-C QrnFunctionKey_20 20;
       DCL-C QrnFunctionKey_21 21;
       DCL-C QrnFunctionKey_22 22;
       DCL-C QrnFunctionKey_23 23;
       DCL-C QrnFunctionKey_24 24;
       DCL-C QrnFunctionKey_PRINT 121;
       DCL-C QrnFunctionKey_ROLLUP 122;
       DCL-C QrnFunctionKey_ROLLDOWN 123;
       DCL-C QrnFunctionKey_CLEAR 124;
       DCL-C QrnFunctionKey_HELP 125;
       DCL-C QrnFunctionKey_HOME 126;

       // How character data's CCSIDs are handled, in alphaCcsids.
       DCL-C QrnCcsids_JOB 0;
       DCL-C QrnCcsids_FILE 1;
       DCL-C QrnCcsids_N_A 2;

       // A printer's spacing and skipping.
       DCL-DS QrnPrtctl_T QUALIFIED TEMPLATE;
          spaceBefore ZONED(3:0);
          spaceAfter ZONED(3:0);
          skipBefore ZONED(3:0);
          skipAfter ZONED(3:0);
          currLine ZONED(3:0);
       END-DS;

       // A record format and its level identifier.
       DCL-DS QrnRecordLevel_T QUALIFIED TEMPLATE;
          record CHAR(10);
          level CHAR(13);
       END-DS;

       DCL-DS QrnRecordLevels_T QUALIFIED TEMPLATE ALIGN;
          num INT(10);
          levels LIKEDS(QrnRecordLevel_T) DIM(32767);
       END-DS;

       // An object and its library.
       DCL-DS QrnObject_T QUALIFIED TEMPLATE;
          name CHAR(10);
          library CHAR(10);
       END-DS;

       // One field of the record as a name and a value in text.
       DCL-DS QrnNameValue_T QUALIFIED TEMPLATE;
          externalName CHAR(10);
          // QrnDatatype_*.
          dataType UNS(3);
          // Digits of a decimal, bytes of an integer or a float.
          numericDefinedLen UNS(3);
          decimals UNS(3);
          // QrnDtzFormat_*.
          dtzFormat UNS(3);
          dtSeparator CHAR(1);
          // The handler sets it (input) or uses it (output).
          input IND;
          output IND;
          isNullCapable IND;
          hasNullValue IND;
          reserved1 CHAR(13);
          valueLenBytes UNS(10);
          valueMaxLenBytes UNS(10);
          valueCcsid INT(10);
          reserved2 UNS(10);
          value POINTER;
          reserved3 POINTER;
       END-DS;

       DCL-DS QrnNamesValues_T QUALIFIED TEMPLATE ALIGN;
          num INT(10);
          field LIKEDS(QrnNameValue_T) DIM(32767);
       END-DS;

       // The handler's one parameter.
       DCL-DS QrnOpenAccess_T TEMPLATE QUALIFIED ALIGN;
          length UNS(10);
          reserved1 UNS(10);
          // ROAC0100.
          parameterFormat CHAR(8);
          // The second operand of HANDLER.
          userArea POINTER;
          // The handler's own, kept from operation to operation.
          stateInfo POINTER;
          // Null for a program-described file.
          recordLevels POINTER;
          // The record buffers and null maps, when the operation uses them.
          inputBuffer POINTER;
          inputNullMap POINTER;
          outputBuffer POINTER;
          outputNullMap POINTER;
          // The fields as names and values (useNamesValues).
          namesValues POINTER;
          // The key, its null map and the key fields as names and values.
          key POINTER;
          keyNullMap POINTER;
          keynamesValues POINTER;
          // The indicators, 99 of them.
          indara POINTER;
          // QrnPrtctl_T for a printer.
          prtctl POINTER;
          // Feedback areas the handler may set.
          openFeedback POINTER;
          ioFeedback POINTER;
          deviceFeedback POINTER;
          reserved2 POINTER;
          reserved3 POINTER;
          reserved4 POINTER;
          externalFile LIKEDS(QrnObject_T);
          externalMember CHAR(10);
          compileFile LIKEDS(QrnObject_T);
          recordName CHAR(10);
          reserved5 CHAR(20);
          // QrnOperation_*.
          rpgOperation UNS(10);
          // The handler's status for the operation.
          rpgStatus INT(10);
          inputBufferLen UNS(10);
          inputNullMapLen UNS(10);
          outputBufferLen UNS(10);
          outputNullMapLen UNS(10);
          keyLen UNS(10);
          keyNullMapLen UNS(10);
          inputDataLen UNS(10);
          openFeedbackLen UNS(10);
          ioFeedbackLen UNS(10);
          deviceFeedbackLen UNS(10);
          numKeys UNS(10);
          rrn UNS(10);
          formLen UNS(10);
          formOfl UNS(10);
          sln UNS(10);
          // QrnCcsids_*.
          alphaCcsids UNS(10);
          reserved8 UNS(10);
          reserved9 UNS(10);
          // QrnFunctionKey_*.
          functionKey UNS(3);
          externallyDescribed IND;
          keyedFile IND;
          blocked IND;
          // Set by the handler.
          eof IND;
          found IND;
          equal IND;
          printerOverflow IND;
          inputWithLock IND;
          // Set on at OPEN to work with namesValues instead of buffers.
          useNamesValues IND;
          isSubfile IND;
          reservedInds IND DIM(3);
          canHandleCcsids IND;
          DCL-SUBF commit IND;
          // QrnDevice_*.
          rpgDevice CHAR(1);
          reserved10 CHAR(31);
       END-DS;

































































































































































































































































