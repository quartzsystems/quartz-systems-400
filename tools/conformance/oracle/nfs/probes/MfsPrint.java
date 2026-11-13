// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.*;
import java.io.*;
import java.nio.charset.Charset;
import java.util.*;

/**
 * DSPMFSINF OUTPUT(*PRINT) on the primary, read back. Args: OUTDIR PATH...
 * For each path: the command, the newest QP0LCDMN spooled file of the
 * user (in its QPRTJOB) copied to QSTEST/NFSPRT (CPYSPLF *FCFC), the member read as CP037
 * text into OUTDIR, then the spooled file and the file deleted. Nothing
 * else is changed.
 */
public final class MfsPrint {
    public static void main(String[] a) throws Exception {
        AS400 s = new AS400(System.getenv("QS_ORACLE_HOST"), System.getenv("QS_ORACLE_USER"), System.getenv("QS_ORACLE_PASS"));
        CommandCall cc = new CommandCall(s);
        for (int i = 1; i < a.length; i++) {
            String path = a[i];
            run(cc, "DSPMFSINF OBJ('" + path + "') OUTPUT(*PRINT)");
            SpooledFileList l = new SpooledFileList(s);
            l.setUserFilter("*CURRENT");
            l.openSynchronously();
            SpooledFile last = null;
            for (int k = 0; k < l.size(); k++) {
                SpooledFile f = (SpooledFile) l.getObject(k);
                // The listing goes to the user's QPRTJOB, not the server job.
                if (f.getName().equals("QP0LCDMN") && (last == null || f.getNumber() > last.getNumber())) last = f;
            }
            l.close();
            if (last == null) { System.out.println(path + ": no spooled file"); continue; }
            System.out.println(path + ": spooled file " + last.getName() + " " + last.getNumber() + " pages " + last.getIntegerAttribute(PrintObject.ATTR_PAGES)
                + " prtf " + last.getStringAttribute(PrintObject.ATTR_PRINTER_FILE) + " lpi " + last.getFloatAttribute(PrintObject.ATTR_LPI)
                + " cpi " + last.getFloatAttribute(PrintObject.ATTR_CPI) + " pagelen " + last.getFloatAttribute(PrintObject.ATTR_PAGELEN)
                + " pagewidth " + last.getFloatAttribute(PrintObject.ATTR_PAGEWIDTH));
            run(cc, "DLTF FILE(QSTEST/NFSPRT)");
            run(cc, "CRTPF FILE(QSTEST/NFSPRT) RCDLEN(199)");
            run(cc, "CPYSPLF FILE(" + last.getName() + ") TOFILE(QSTEST/NFSPRT) JOB(" + last.getJobNumber() + "/" + last.getJobUser() + "/" + last.getJobName()
                + ") SPLNBR(" + last.getNumber() + ") CTLCHAR(*FCFC)");
            IFSFile f = new IFSFile(s, "/QSYS.LIB/QSTEST.LIB/NFSPRT.FILE/NFSPRT.MBR");
            StringBuilder sb = new StringBuilder();
            try (Reader in = new InputStreamReader(new IFSFileInputStream(f, IFSFileInputStream.SHARE_ALL), Charset.forName("Cp037"))) {
                char[] buf = new char[65536];
                int n;
                while ((n = in.read(buf)) > 0) sb.append(buf, 0, n);
            }
            String name = path.equals("/") ? "root" : path.replaceAll("[^A-Za-z0-9]", "").toLowerCase();
            try (Writer o = new OutputStreamWriter(new FileOutputStream(a[0] + "/dspmfsinf-" + name + "-print.txt"), "UTF-8")) {
                String t = sb.toString();
                for (int p = 0; p + 199 <= t.length(); p += 199) o.write(t.substring(p, p + 199).replaceAll("\\s+$", "") + "\n");
            }
            last.delete();
            run(cc, "DLTF FILE(QSTEST/NFSPRT)");
        }
        s.disconnectAllServices();
    }

    static void run(CommandCall cc, String c) throws Exception {
        boolean ok = cc.run(c);
        StringBuilder m = new StringBuilder();
        for (AS400Message x : cc.getMessageList()) m.append(x.getID()).append(' ');
        System.out.println((ok ? "ok  " : "ERR ") + c + " : " + m);
    }
}
