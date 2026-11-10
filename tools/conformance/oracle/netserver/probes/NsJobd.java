// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.*;
import java.io.*;
import java.nio.charset.Charset;
import java.util.*;

/** Read-only: QWDRJOBD JOBD0100 of QSYS/QZLSPJ and QSYS/QZLSSERVER on the primary. Arg: output file. */
public final class NsJobd {
    static final AS400Bin4 B = new AS400Bin4();
    static final Charset E = Charset.forName("Cp037");

    static byte[] e(String v, int n) {
        byte[] b = new byte[n];
        Arrays.fill(b, (byte) 0x40);
        byte[] x = v.getBytes(E);
        System.arraycopy(x, 0, b, 0, Math.min(n, x.length));
        return b;
    }

    public static void main(String[] a) throws Exception {
        AS400 s = new AS400(System.getenv("QS_ORACLE_HOST"), System.getenv("QS_ORACLE_USER"), System.getenv("QS_ORACLE_PASS"));
        s.setGuiAvailable(false);
        try (PrintWriter w = new PrintWriter(new FileWriter(a[0]))) {
            w.println("# QWDRJOBD JOBD0100 on the primary (V7R1M0), 2026-11-09. Text descriptions are IBM's and not kept.");
            for (String jd : new String[] {"QZLSPJ", "QZLSSERVER"}) {
                int n = 4096;
                ProgramParameter[] p = {
                    new ProgramParameter(n), new ProgramParameter(B.toBytes(n)), new ProgramParameter(e("JOBD0100", 8)),
                    new ProgramParameter(e(jd, 10 ) ), new ProgramParameter(new byte[8], 8),
                };
                byte[] q = new byte[20];
                System.arraycopy(e(jd, 10), 0, q, 0, 10);
                System.arraycopy(e("QSYS", 10), 0, q, 10, 10);
                p[3] = new ProgramParameter(q);
                ProgramCall c = new ProgramCall(s, "/QSYS.LIB/QWDRJOBD.PGM", p);
                if (!c.run()) { w.println(jd + " failed"); continue; }
                byte[] r = p[0].getOutputData();
                int ret = B.toInt(r, 0);
                w.println("== QSYS/" + jd + " (" + ret + " bytes)");
                String[][] f = {
                    {"USER", "28", "10"}, {"JOBDATE", "38", "8"}, {"JOBSWS", "46", "8"}, {"JOBQ", "54", "10"}, {"JOBQ library", "64", "10"},
                    {"JOBPTY", "74", "2"}, {"HOLD", "76", "10"}, {"OUTQ", "86", "10"}, {"OUTQ library", "96", "10"}, {"OUTPTY", "106", "2"},
                    {"PRTDEV", "108", "10"}, {"PRTTXT", "118", "30"}, {"LOG level", "160", "1"}, {"LOG text", "161", "10"}, {"LOGCLPGM", "171", "10"},
                    {"INQMSGRPY", "181", "10"}, {"DEVRCYACN", "191", "13"}, {"TSEPOOL", "204", "10"}, {"ACGCDE", "214", "15"}, {"RTGDTA", "229", "80"},
                    {"JOBMSGQFL", "380", "10"}, {"(390)", "390", "10"}, {"(400)", "400", "10"}, {"(410)", "410", "10"}, {"(432)", "432", "10"}, {"(442)", "442", "10"},
                };
                for (String[] x : f) {
                    w.println(String.format("  %-14s '%s'", x[0], new String(r, Integer.parseInt(x[1]), Integer.parseInt(x[2]), E).replaceAll("\\s+$", "")));
                }
                w.println("  SYNTAX         " + B.toInt(r, 148) + "   ENDSEV " + B.toInt(r, 152) + "   LOG severity " + B.toInt(r, 156));
                w.println("  JOBMSGQMX      " + B.toInt(r, 376));
                int lo = B.toInt(r, 360), lc = B.toInt(r, 364), ro = B.toInt(r, 368), rl = B.toInt(r, 372);
                StringBuilder l = new StringBuilder();
                for (int i = 0; i < lc; i++) l.append(new String(r, lo + 11 * i, 10, E).trim()).append(' ');
                w.println("  INLLIBL        " + l.toString().trim());
                w.println("  RQSDTA         '" + new String(r, ro, rl, E) + "'");
            }
        }
        s.disconnectAllServices();
    }
}
