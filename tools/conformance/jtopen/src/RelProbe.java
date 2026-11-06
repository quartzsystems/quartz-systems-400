// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;
import com.ibm.as400.access.AS400Bin4;
import com.ibm.as400.access.AS400Message;
import com.ibm.as400.access.AS400PackedDecimal;
import com.ibm.as400.access.AS400Text;
import com.ibm.as400.access.MessageFile;
import com.ibm.as400.access.ProgramCall;
import com.ibm.as400.access.ProgramParameter;

/**
 * Read-only release probe (V1R1M0 items D1, D6, D7): system values through QWCRSVAL
 * (type, length, value), message descriptions of QCPFMSG (severity and the number of
 * substitution variables; the text goes to stderr only, never into a capture), and
 * command definitions through QCDRCMDI (CDML, to stderr). Usage:
 * RelProbe sysval NAME... | msg ID... | cmd NAME...
 * Credentials: QS_HS_HOST, QS_HS_USER, QS_HS_PASS.
 */
public final class RelProbe {
    static final AS400Bin4 BIN4 = new AS400Bin4();

    public static void main(String[] a) throws Exception {
        AS400 s = Hs.system();
        String[] names = java.util.Arrays.copyOfRange(a, 1, a.length);
        switch (a[0]) {
            case "sysval" -> {
                for (String n : names) {
                    sysval(s, n);
                }
            }
            case "msg" -> {
                MessageFile f = new MessageFile(s, "/QSYS.LIB/QCPFMSG.MSGF");
                for (String id : names) {
                    try {
                        AS400Message m = f.getMessage(id);
                        String t = m.getText();
                        int vars = 0;
                        for (int i = 1; i < 100; i++) {
                            if (t.contains("&" + i) || (m.getHelp() != null && m.getHelp().contains("&" + i))) {
                                vars = i;
                            }
                        }
                        System.out.println(id + " sev=" + m.getSeverity() + " vars>=" + vars);
                        System.err.println(id + " TEXT " + t);
                        System.err.println(id + " HELP " + m.getHelp());
                    } catch (Exception e) {
                        System.out.println(id + " missing: " + e.getMessage());
                    }
                }
            }
            case "cmd" -> {
                for (String n : names) {
                    cmd(s, n);
                }
            }
            default -> throw new IllegalArgumentException(a[0]);
        }
        s.disconnectAllServices();
    }

    static void sysval(AS400 s, String name) throws Exception {
        AS400Text name10 = new AS400Text(10, s);
        ProgramParameter[] p = {
            new ProgramParameter(2000),
            new ProgramParameter(BIN4.toBytes(2000)),
            new ProgramParameter(BIN4.toBytes(1)),
            new ProgramParameter(name10.toBytes(name)),
            new ProgramParameter(new byte[8], 8),
        };
        ProgramCall c = new ProgramCall(s, "/QSYS.LIB/QWCRSVAL.PGM", p);
        if (!c.run()) {
            for (AS400Message m : c.getMessageList()) {
                System.out.println(name + " refused " + m.getID());
            }
            return;
        }
        byte[] r = p[0].getOutputData();
        int off = BIN4.toInt(r, 4);
        String type = (String) new AS400Text(1, s).toObject(r, off + 10);
        String status = (String) new AS400Text(1, s).toObject(r, off + 11);
        int len = BIN4.toInt(r, off + 12);
        String v;
        if (type.equals("B")) {
            v = Integer.toString(BIN4.toInt(r, off + 16));
        } else if (type.equals("P")) {
            v = new AS400PackedDecimal(len * 2 - 1, 4).toObject(r, off + 16).toString();
        } else {
            v = "'" + ((String) new AS400Text(len, s).toObject(r, off + 16)) + "'";
        }
        System.out.println(name + " type=" + type + " status=" + status + " len=" + len + " value=" + v);
    }

    static void cmd(AS400 s, String name) throws Exception {
        AS400Text t20 = new AS400Text(20, s);
        int size = 200000;
        ProgramParameter[] p = {
            new ProgramParameter(t20.toBytes(String.format("%-10s%-10s", name, "QSYS"))),
            new ProgramParameter(BIN4.toBytes(size)),
            new ProgramParameter(new AS400Text(8, s).toBytes("DEST0100")),
            new ProgramParameter(size),
            new ProgramParameter(new AS400Text(8, s).toBytes("CMDD0100")),
            new ProgramParameter(new byte[8], 8),
        };
        ProgramCall c = new ProgramCall(s, "/QSYS.LIB/QCDRCMDD.PGM", p);
        if (!c.run()) {
            for (AS400Message m : c.getMessageList()) {
                System.out.println(name + " refused " + m.getID());
            }
            return;
        }
        byte[] r = p[3].getOutputData();
        int avail = BIN4.toInt(r, 4);
        String xml = new String(r, 8, Math.min(avail, size - 8), "UTF-8");
        System.out.println(name + " cdml bytes=" + avail);
        System.err.println("=== " + name);
        System.err.println(xml);
    }
}
