// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;
import com.ibm.as400.access.AS400Bin4;
import com.ibm.as400.access.AS400Text;
import com.ibm.as400.access.ProgramCall;
import com.ibm.as400.access.ProgramParameter;

/** Distributed program call: QWCRSVAL and QUSRJOBI with marshalled parameters, and a missing program. */
public final class PgmCall {
    static final AS400Bin4 BIN4 = new AS400Bin4();

    public static void main(String[] a) {
        AS400 s = Hs.system();
        Hs.step("connect", () -> s.connectService(AS400.COMMAND));
        Hs.step("QWCRSVAL", () -> qwcrsval(s));
        Hs.step("QUSRJOBI", () -> qusrjobi(s));
        Hs.step("missing program", () -> {
            ProgramCall p = new ProgramCall(s, "/QSYS.LIB/QSNOPGM1.PGM", new ProgramParameter[0]);
            Hs.out("     result=" + p.run());
            Hs.messages("msg", p.getMessageList());
        });
        s.disconnectAllServices();
    }

    static byte[] errorCode() {
        return new byte[8];
    }

    static void qwcrsval(AS400 s) throws Exception {
        String[] names = {"QSECURITY", "QPWDLVL", "QCCSID", "QMODEL"};
        AS400Text name10 = new AS400Text(10, s);
        byte[] list = new byte[10 * names.length];
        for (int i = 0; i < names.length; i++) {
            name10.toBytes(names[i], list, 10 * i);
        }
        ProgramParameter[] p = {
            new ProgramParameter(400),
            new ProgramParameter(BIN4.toBytes(400)),
            new ProgramParameter(BIN4.toBytes(names.length)),
            new ProgramParameter(list),
            new ProgramParameter(errorCode(), 8),
        };
        ProgramCall c = new ProgramCall(s, "/QSYS.LIB/QWCRSVAL.PGM", p);
        boolean ok = c.run();
        Hs.out("     result=" + ok);
        Hs.messages("msg", c.getMessageList());
        if (!ok) {
            return;
        }
        byte[] r = p[0].getOutputData();
        int n = BIN4.toInt(r, 0);
        for (int i = 0; i < n; i++) {
            int off = BIN4.toInt(r, 4 + 4 * i);
            String nm = (String) name10.toObject(r, off);
            char type = (char) new AS400Text(1, s).toObject(r, off + 10).toString().charAt(0);
            int len = BIN4.toInt(r, off + 12);
            String v = type == 'B' ? Integer.toString(BIN4.toInt(r, off + 16))
                : ((String) new AS400Text(len, s).toObject(r, off + 16)).trim();
            Hs.out("     " + nm.trim() + " type=" + type + " len=" + len + " value=" + v);
        }
    }

    static void qusrjobi(AS400 s) throws Exception {
        ProgramParameter[] p = {
            new ProgramParameter(86),
            new ProgramParameter(BIN4.toBytes(86)),
            new ProgramParameter(new AS400Text(8, s).toBytes("JOBI0100")),
            new ProgramParameter(new AS400Text(26, s).toBytes("*")),
            new ProgramParameter(new AS400Text(16, s).toBytes("")),
            new ProgramParameter(errorCode(), 8),
        };
        ProgramCall c = new ProgramCall(s, "/QSYS.LIB/QUSRJOBI.PGM", p);
        boolean ok = c.run();
        Hs.out("     result=" + ok);
        Hs.messages("msg", c.getMessageList());
        if (!ok) {
            return;
        }
        byte[] r = p[0].getOutputData();
        Hs.out("     returned=" + BIN4.toInt(r, 0) + " available=" + BIN4.toInt(r, 4));
        Hs.out("     job name=" + ((String) new AS400Text(10, s).toObject(r, 8)).trim()
            + " type=" + new AS400Text(1, s).toObject(r, 60));
    }
}
