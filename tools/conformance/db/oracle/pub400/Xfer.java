// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;
import com.ibm.as400.access.AS400Message;
import com.ibm.as400.access.CharConverter;
import com.ibm.as400.access.CommandCall;
import com.ibm.as400.access.UserSpace;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * File transfer and CL over the remote command server only.
 *   Xfer HOST USER PASS script FILE
 * Script lines (tab separated): cmd<TAB>CL [<TAB>stop]
 *                                put<TAB>LOCAL<TAB>/remote/path
 *                                get<TAB>/remote/path<TAB>LOCAL
 * put writes text through QShell printf; get runs QGPL/USGET and reads
 * user space QGPL/XFER.
 */
public class Xfer {
    static AS400 sys;
    static CommandCall cc;

    public static void main(String[] a) throws Exception {
        sys = new AS400(a[0], a[1], a[2].toCharArray());
        sys.setGuiAvailable(false);
        sys.connectService(AS400.COMMAND);
        cc = new CommandCall(sys);
        cmd("ADDENVVAR ENVVAR(QIBM_QSH_CMD_OUTPUT) VALUE(NONE)", false);
        cmd("CHGJOB INQMSGRPY(*DFT)", false);
        int rc = 0;
        for (String line : Files.readAllLines(Path.of(a[4]), StandardCharsets.UTF_8)) {
            if (line.isBlank() || line.startsWith("#")) continue;
            String[] p = line.split("\t");
            switch (p[0]) {
                case "cmd" -> {
                    boolean ok = cmd(p[1], true);
                    if (!ok && p.length > 2 && p[2].equals("stop")) { System.out.println("stopped"); rc = 1; }
                }
                case "put" -> put(p[1], p[2]);
                case "iput" -> iput(p[1], p[2]);
                case "iget" -> iget(p[1], p[2]);
                case "get" -> get(p[1], p[2]);
                case "log" -> log(Integer.parseInt(p[1]));
                case "da" -> {
                    com.ibm.as400.access.CharacterDataArea d = new com.ibm.as400.access.CharacterDataArea(sys, "/QSYS.LIB/QGPL.LIB/XFERDA.DTAARA");
                    String v = d.read();
                    int e = v.length(); while (e > 0 && v.charAt(e - 1) == ' ') e--;
                    System.out.println("da: " + v.substring(0, e).replace("|", "\n      "));
                }
                default -> System.out.println("?? " + line);
            }
            if (rc != 0) break;
        }
        sys.disconnectAllServices();
        System.exit(rc);
    }

    static void log(int last) throws Exception {
        com.ibm.as400.access.Job j = cc.getServerJob();
        com.ibm.as400.access.JobLog jl = new com.ibm.as400.access.JobLog(sys, j.getName(), j.getUser(), j.getNumber());
        jl.addAttributeToRetrieve(com.ibm.as400.access.JobLog.MESSAGE_WITH_REPLACEMENT_DATA);
        jl.addAttributeToRetrieve(com.ibm.as400.access.JobLog.MESSAGE_HELP_WITH_REPLACEMENT_DATA);
        jl.setListDirection(false);
        jl.load();
        Object[] ms = jl.getMessages(0, last);
        for (int i = ms.length - 1; i >= 0; i--) {
            com.ibm.as400.access.QueuedMessage m = (com.ibm.as400.access.QueuedMessage) ms[i];
            System.out.println("  log " + m.getID() + " " + m.getText());
        }
    }

    static boolean cmd(String c, boolean show) throws Exception {
        boolean ok = cc.run(c);
        if (show) {
            System.out.println((ok ? "OK  " : "ERR ") + c);
            for (AS400Message m : cc.getMessageList()) System.out.println("    " + m.getID() + " " + m.getText());
        }
        return ok;
    }

    /** One line as a single-quoted QShell argument, with CL quote doubling. */
    static String esc(String s) {
        String q = s.replace("'", "'\\''");
        return (" '" + q + "'").replace("'", "''");
    }

    /** Upload through the file server as UTF-8, tagged 1208. */
    static void iput(String local, String remote) throws Exception {
        com.ibm.as400.access.IFSFile f = new com.ibm.as400.access.IFSFile(sys, remote);
        try (com.ibm.as400.access.IFSFileOutputStream o = new com.ibm.as400.access.IFSFileOutputStream(sys, remote)) {
            o.write(Files.readAllBytes(Path.of(local)));
        }
        f.setCCSID(1208);
        System.out.println("iput " + remote + " " + f.length() + " bytes");
    }

    /** Download through the file server, converting from the file's CCSID. */
    static void iget(String remote, String local) throws Exception {
        com.ibm.as400.access.IFSFile f = new com.ibm.as400.access.IFSFile(sys, remote);
        byte[] b;
        try (com.ibm.as400.access.IFSFileInputStream i = new com.ibm.as400.access.IFSFileInputStream(sys, remote)) {
            b = i.readAllBytes();
        }
        int ccsid = f.getCCSID();
        if (System.getenv("RAWGET") != null) {
            Files.write(Path.of(local), b);
            System.out.println("iget raw " + remote + " ccsid " + ccsid + " " + b.length + " bytes -> " + local);
            return;
        }
        String text = ccsid == 1208 || ccsid == 819 ? new String(b, ccsid == 1208 ? StandardCharsets.UTF_8 : StandardCharsets.ISO_8859_1)
            : new CharConverter(ccsid).byteArrayToString(b);
        Files.writeString(Path.of(local), text.replace("\r\n", "\n"), StandardCharsets.UTF_8);
        System.out.println("iget " + remote + " ccsid " + ccsid + " " + b.length + " bytes -> " + local);
    }

    static void put(String local, String remote) throws Exception {
        List<String> lines = Files.readAllLines(Path.of(local), StandardCharsets.UTF_8);
        cmd("QSH CMD('rm -f " + remote + "')", false);
        StringBuilder batch = new StringBuilder();
        int n = 0, calls = 0;
        for (String l : lines) {
            String e = esc(l);
            if (e.length() > 2800) throw new RuntimeException("line too long in " + local);
            if (batch.length() + e.length() > 2800) {
                flush(batch, remote); calls++; batch.setLength(0);
            }
            batch.append(e); n++;
        }
        if (batch.length() > 0) { flush(batch, remote); calls++; }
        System.out.println("put " + remote + " " + n + " lines in " + calls + " calls");
    }

    static void flush(StringBuilder fmt, String remote) throws Exception {
        String c = "QSH CMD('printf ''%s\\n''" + fmt + " >> " + remote + "')";
        if (!cmd(c, false)) {
            System.out.println("ERR put batch to " + remote);
            for (AS400Message m : cc.getMessageList()) System.out.println("    " + m.getID() + " " + m.getText());
            throw new RuntimeException("put failed");
        }
    }

    static void get(String remote, String local) throws Exception {
        if (!cmd("CHGDTAARA DTAARA(QGPL/XFERDA) VALUE('" + remote + "')", false)) throw new RuntimeException("get failed");
        if (!cmd("CALL QGPL/USGET", true)) throw new RuntimeException("get failed");
        UserSpace us = new UserSpace(sys, "/QSYS.LIB/QGPL.LIB/XFER.USRSPC");
        us.setMustUseProgramCall(true);
        CharConverter cv = new CharConverter(37);
        StringBuilder out = new StringBuilder();
        byte[] rec = new byte[1001];
        int pos = 0, n = 0;
        while (true) {
            int got = us.read(rec, pos);
            if (got < 1001 || rec[0] == 0) break;
            String s = cv.byteArrayToString(rec, 0, 1000);
            int end = s.length();
            while (end > 0 && s.charAt(end - 1) == ' ') end--;
            out.append(s, 0, end).append('\n');
            pos += 1001; n++;
        }
        Files.writeString(Path.of(local), out.toString(), StandardCharsets.UTF_8);
        System.out.println("get " + remote + " " + n + " lines -> " + local);
    }
}
