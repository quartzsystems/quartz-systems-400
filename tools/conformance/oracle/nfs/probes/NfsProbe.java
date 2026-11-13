// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.*;
import java.io.*;
import java.net.*;
import java.nio.file.*;
import java.sql.*;
import java.util.*;

/**
 * Read-only NFS probe. Args: primary|pub400 OUTDIR. Only QTEMP is written
 * (on pub400 nothing: the object lists and the wider message scan are skipped).
 * Message text and command XML go to OUTDIR (kept out of the tree); the
 * summary on stdout holds IDs, severities and names only.
 */
public final class NfsProbe {
    static String host, user, pass;
    static Connection c;
    static Path out;

    static void q(String title, String sql) {
        System.out.println("== " + title);
        try (Statement s = c.createStatement(); ResultSet r = s.executeQuery(sql)) {
            ResultSetMetaData m = r.getMetaData();
            StringBuilder h = new StringBuilder("   |");
            for (int i = 1; i <= m.getColumnCount(); i++) h.append(m.getColumnName(i)).append('|');
            System.out.println(h);
            int n = 0;
            while (r.next() && n < 500) {
                StringBuilder b = new StringBuilder("   |");
                for (int i = 1; i <= m.getColumnCount(); i++) { String v = r.getString(i); b.append(v == null ? "-" : v.trim()).append('|'); }
                System.out.println(b);
                n++;
            }
            System.out.println("   rows=" + n);
        } catch (Exception e) { System.out.println("   ERR " + e.getMessage()); }
    }

    static void x(String sql) {
        try (Statement s = c.createStatement()) { s.execute(sql); } catch (Exception e) { System.out.println("   ERR(x) " + sql + " : " + e.getMessage()); }
    }

    static void cmd(String cl) {
        String esc = cl.replace("'", "''");
        x(String.format("CALL QSYS.QCMDEXC('%s', %015.5f)", esc, (double) cl.length()));
    }

    static void port(int p) {
        try (Socket s = new Socket()) {
            s.connect(new InetSocketAddress(host, p), 4000);
            System.out.println("   tcp " + p + " open");
        } catch (Exception e) { System.out.println("   tcp " + p + " " + e.getMessage()); }
    }

    static void file(AS400 s, String path) {
        try {
            IFSFile f = new IFSFile(s, path);
            if (!f.exists()) { System.out.println("   " + path + " absent"); return; }
            System.out.println("   " + path + " len=" + f.length() + " ccsid=" + f.getCCSID() + " owner=" + f.getOwnerName() + " dir=" + f.isDirectory());
            if (f.isDirectory()) {
                String[] l = f.list();
                if (l != null) for (String n : l) System.out.println("      " + n);
                return;
            }
            byte[] b = new byte[(int) Math.min(f.length(), 65536)];
            try (IFSFileInputStream in = new IFSFileInputStream(s, path)) { int n = in.read(b); if (n < 0) n = 0; b = Arrays.copyOf(b, n); }
            Files.write(out.resolve(path.replace('/', '_') + ".bin"), b);
        } catch (Exception e) { System.out.println("   " + path + " ERR " + e); }
    }

    public static void main(String[] a) throws Exception {
        boolean pub = a[0].equals("pub400");
        out = Paths.get(a[1]);
        Files.createDirectories(out);
        host = System.getenv(pub ? "QS_PUB400_HOST" : "QS_ORACLE_HOST");
        user = System.getenv(pub ? "QS_PUB400_USER" : "QS_ORACLE_USER");
        pass = System.getenv(pub ? "QS_PUB400_PASS" : "QS_ORACLE_PASS");
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        c = DriverManager.getConnection("jdbc:as400://" + host + ";naming=sql;prompt=false;errors=full", user, pass);
        AS400 s = new AS400(host, user, pass);

        System.out.println("== ports");
        for (int p : new int[] {111, 2049}) port(p);

        if (!pub) cmd("DSPOBJD OBJ(QSYS/*ALL) OBJTYPE(*ALL) OUTPUT(*OUTFILE) OUTFILE(QTEMP/SOBJ)");
        if (!pub) q("QSYS objects about NFS/RPC", "SELECT ODOBNM, ODOBTP, ODOBAT, ODOBTX FROM QTEMP.SOBJ WHERE ODOBNM LIKE 'QNFS%' OR ODOBNM LIKE 'QP0N%' OR ODOBNM LIKE 'QP0F%' OR ODOBNM LIKE 'QZNF%' OR UPPER(ODOBTX) LIKE '%NFS%' OR UPPER(ODOBTX) LIKE '%NETWORK FILE%' OR UPPER(ODOBTX) LIKE '%RPC%' OR UPPER(ODOBTX) LIKE '%MOUNT%' OR UPPER(ODOBTX) LIKE '%EXPORT%' ORDER BY 2,1");
        if (!pub) cmd("DSPOBJD OBJ(QUSRSYS/*ALL) OBJTYPE(*ALL) OUTPUT(*OUTFILE) OUTFILE(QTEMP/UOBJ)");
        if (!pub) q("QUSRSYS objects about NFS/RPC", "SELECT ODOBNM, ODOBTP, ODOBAT, ODOBTX FROM QTEMP.UOBJ WHERE ODOBNM LIKE 'QNFS%' OR ODOBNM LIKE 'QP0N%' OR UPPER(ODOBTX) LIKE '%NFS%' OR UPPER(ODOBTX) LIKE '%RPC%'");
        q("QSYSINC/H members", "SELECT SYSTEM_TABLE_MEMBER, TABLE_NAME FROM QSYS2.SYSPARTITIONSTAT WHERE TABLE_SCHEMA='QSYSINC' AND (SYSTEM_TABLE_MEMBER LIKE '%NFS%' OR SYSTEM_TABLE_MEMBER LIKE '%RPC%' OR SYSTEM_TABLE_MEMBER LIKE '%MOUNT%' OR SYSTEM_TABLE_MEMBER LIKE '%STATVFS%' OR SYSTEM_TABLE_MEMBER LIKE 'QP0L%' OR SYSTEM_TABLE_MEMBER LIKE 'QZNF%')");
        q("QSYS2 routines", "SELECT ROUTINE_SCHEMA, ROUTINE_NAME, ROUTINE_TYPE FROM QSYS2.SYSROUTINES WHERE ROUTINE_NAME LIKE '%NFS%' OR ROUTINE_NAME LIKE '%MOUNT%' OR ROUTINE_NAME LIKE '%EXPORT%' OR ROUTINE_NAME LIKE '%FILE_SYSTEM%' ORDER BY 1,2");
        q("QSYS2 views", "SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA IN ('QSYS2','SYSTOOLS','SYSIBMADM') AND (TABLE_NAME LIKE '%NFS%' OR TABLE_NAME LIKE '%MOUNT%' OR TABLE_NAME LIKE '%EXPORT%' OR TABLE_NAME LIKE '%FILE_SYSTEM%')");
        q("view columns", "SELECT TABLE_NAME, ORDINAL_POSITION, COLUMN_NAME, SYSTEM_COLUMN_NAME, DATA_TYPE, LENGTH, NUMERIC_SCALE, IS_NULLABLE FROM QSYS2.SYSCOLUMNS WHERE TABLE_SCHEMA IN ('QSYS2','SYSTOOLS') AND (TABLE_NAME LIKE '%NFS%' OR TABLE_NAME LIKE '%MOUNT%' OR TABLE_NAME LIKE '%EXPORT%' OR TABLE_NAME LIKE '%FILE_SYSTEM%') ORDER BY 1,2");
        q("routine parms", "SELECT SPECIFIC_NAME, ORDINAL_POSITION, PARAMETER_MODE, PARAMETER_NAME, DATA_TYPE, CHARACTER_MAXIMUM_LENGTH FROM QSYS2.SYSPARMS WHERE SPECIFIC_SCHEMA IN ('QSYS2','SYSTOOLS') AND (SPECIFIC_NAME LIKE '%NFS%' OR SPECIFIC_NAME LIKE '%MOUNT%' OR SPECIFIC_NAME LIKE '%EXPORT%' OR SPECIFIC_NAME LIKE '%FILE_SYSTEM%') ORDER BY 1,2");
        q("exit points", "SELECT EXIT_POINT_NAME, EXIT_POINT_FORMAT FROM QSYS2.EXIT_POINT_INFO WHERE EXIT_POINT_NAME LIKE '%NFS%' OR EXIT_POINT_NAME LIKE '%RPC%' OR EXIT_POINT_NAME LIKE '%QP0L%' OR EXIT_POINT_NAME LIKE '%QSTRT%'");
        q("function usage", "SELECT FUNCTION_ID FROM QSYS2.FUNCTION_INFO WHERE FUNCTION_ID LIKE '%NFS%' OR FUNCTION_ID LIKE '%RPC%'");
        q("tcpip servers", "SELECT * FROM QSYS2.TCPIP_SERVER_INFO WHERE SERVER_NAME LIKE '%NFS%' OR SERVER_NAME LIKE '%RPC%'");

        System.out.println("== messages (id sev); text in OUTDIR/messages.txt");
        MessageFile mf = new MessageFile(s, "/QSYS.LIB/QCPFMSG.MSGF");
        mf.setHelpTextFormatting(MessageFile.NO_FORMATTING);
        String hex = "0123456789ABCDEF";
        try (PrintWriter pw = new PrintWriter(Files.newBufferedWriter(out.resolve("messages.txt")))) {
            for (String pre : pub ? new String[] {"CPFA1", "CPDA1", "CPIA1", "CPCA1"} : new String[] {"CPFA1", "CPDA1", "CPIA1", "CPCA1", "CPFA0", "CPDA0", "CPIA0", "CPCA0"}) {
                for (int i = 0; i < 16; i++) for (int j = 0; j < 16; j++) {
                    String id = pre + hex.charAt(i) + hex.charAt(j);
                    try {
                        AS400Message m = mf.getMessage(id);
                        System.out.println("   " + id + " sev=" + m.getSeverity());
                        pw.println(id + " sev=" + m.getSeverity() + " | " + m.getText());
                        pw.println("      " + m.getHelp());
                    } catch (Exception e) { }
                }
            }
        }

        System.out.println("== commands (XML in OUTDIR)");
        for (String n : new String[] {"STRNFSSVR", "ENDNFSSVR", "EXPORTFS", "CHGNFSEXP", "MOUNT", "ADDMFS", "UNMOUNT", "RMVMFS", "DSPMFSINF", "STATFS", "RLSIFSLCK", "CHGNFSA", "WRKNFSEXP", "DSPNFSEXP", "RPCBIND", "RPCGEN", "ENDRPCBIND", "CVTRPCSRC", "STRTCPSVR"}) {
            try {
                Command cm = new Command(s, "/QSYS.LIB/" + n + ".CMD");
                String xml = cm.getXML();
                Files.writeString(out.resolve(n + ".xml"), xml);
                System.out.println("   " + n + " ok len=" + xml.length() + " cpp=" + cm.getCommandProcessingProgram());
            } catch (Exception e) { System.out.println("   " + n + " ERR " + e.getMessage()); }
        }

        System.out.println("== files");
        for (String p : new String[] {"/etc", "/etc/exports", "/etc/netgroup", "/etc/RPCBTAB", "/etc/STATD", "/etc/netconfig", "/etc/vfs", "/etc/rpc", "/etc/rpcbind", "/etc/pmap", "/etc/xtab", "/etc/rmtab", "/etc/mtab", "/etc/fstab", "/etc/passwd", "/QIBM/ProdData/OS400/RPC", "/QIBM/UserData/OS400/RPC", "/QIBM/UserData/OS400/NFS", "/QIBM/ProdData/OS400/NFS", "/etc/sm", "/etc/sm.bak", "/etc/state"}) file(s, p);

        System.out.println("== jobs QNFS*, QRPC*");
        for (String pat : new String[] {"QNFS*", "QRPC*", "QP0N*"}) {
            try {
                JobList jl = new JobList(s);
                jl.addJobSelectionCriteria(JobList.SELECTION_JOB_NAME, pat);
                Enumeration<?> en = jl.getJobs();
                while (en.hasMoreElements()) { Job j = (Job) en.nextElement(); System.out.println("   " + j.getNumber() + "/" + j.getUser() + "/" + j.getName() + " " + j.getStatus() + " " + j.getSubsystem() + " type=" + j.getType() + j.getSubtype()); }
            } catch (Exception e) { System.out.println("   ERR " + e); }
        }
        s.disconnectAllServices();
        c.close();
    }
}
