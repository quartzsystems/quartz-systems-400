// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.*;
import java.sql.*;
import java.util.*;

/** Read-only NetServer probe. Arg: primary|pub400. Only QTEMP is written. */
public final class NsProbe {
    static String host, user, pass;
    static Connection c;

    static void q(String title, String sql) {
        System.out.println("== " + title);
        System.out.println("   " + sql);
        try (Statement s = c.createStatement(); ResultSet r = s.executeQuery(sql)) {
            ResultSetMetaData m = r.getMetaData();
            StringBuilder h = new StringBuilder("   |");
            for (int i = 1; i <= m.getColumnCount(); i++) h.append(m.getColumnName(i)).append(':').append(m.getColumnTypeName(i)).append('(').append(m.getPrecision(i)).append(")|");
            System.out.println(h);
            int n = 0;
            while (r.next() && n < 400) {
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

    public static void main(String[] a) throws Exception {
        boolean pub = a[0].equals("pub400");
        host = System.getenv(pub ? "QS_PUB400_HOST" : "QS_ORACLE_HOST");
        user = System.getenv(pub ? "QS_PUB400_USER" : "QS_ORACLE_USER");
        pass = System.getenv(pub ? "QS_PUB400_PASS" : "QS_ORACLE_PASS");
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        c = DriverManager.getConnection("jdbc:as400://" + host + ";naming=sql;prompt=false;errors=full", user, pass);

        // Objects named like NetServer's.
        cmd("DSPOBJD OBJ(QSYS/QZLS*) OBJTYPE(*ALL) OUTPUT(*OUTFILE) OUTFILE(QTEMP/ZOBJ)");
        q("QSYS QZLS* objects", "SELECT ODOBNM, ODOBTP, ODOBAT, ODOBTX FROM QTEMP.ZOBJ ORDER BY 1");
        cmd("DSPOBJD OBJ(QUSRSYS/*ALL) OBJTYPE(*ALL) OUTPUT(*OUTFILE) OUTFILE(QTEMP/UOBJ)");
        q("QUSRSYS objects with ZLS/NETS/SHR", "SELECT ODOBNM, ODOBTP, ODOBAT, ODOBTX FROM QTEMP.UOBJ WHERE ODOBNM LIKE '%ZLS%' OR ODOBNM LIKE '%NTS%' OR ODOBNM LIKE '%SHR%' OR ODOBNM LIKE '%SHARE%' OR UPPER(ODOBTX) LIKE '%NETSERVER%' OR UPPER(ODOBTX) LIKE '%SHARE%' ORDER BY 1");
        cmd("DSPOBJD OBJ(QSYS/*ALL) OBJTYPE(*ALL) OUTPUT(*OUTFILE) OUTFILE(QTEMP/SOBJ)");
        q("QSYS objects with NetServer text", "SELECT ODOBNM, ODOBTP, ODOBAT, ODOBTX FROM QTEMP.SOBJ WHERE UPPER(ODOBTX) LIKE '%NETSERVER%' OR UPPER(ODOBTX) LIKE '%NETSVR%' OR UPPER(ODOBTX) LIKE '%SMB%' OR ODOBNM LIKE 'QZLS%' ORDER BY 1");
        q("QSYSINC/H QZLS members", "SELECT SYSTEM_TABLE_MEMBER, PARTITION_TEXT FROM QSYS2.SYSPARTITIONSTAT WHERE TABLE_SCHEMA='QSYSINC' AND SYSTEM_TABLE_MEMBER LIKE 'QZLS%'");

        // SQL catalog.
        q("QSYS2 routines about shares/netserver", "SELECT ROUTINE_SCHEMA, ROUTINE_NAME, ROUTINE_TYPE, SPECIFIC_NAME FROM QSYS2.SYSROUTINES WHERE ROUTINE_NAME LIKE '%SHARE%' OR ROUTINE_NAME LIKE '%NETSERVER%' OR ROUTINE_NAME LIKE '%SERVER_SESSION%' OR ROUTINE_NAME LIKE '%NETSVR%' ORDER BY 1,2");
        q("QSYS2 parms of those routines", "SELECT SPECIFIC_NAME, ORDINAL_POSITION, PARAMETER_MODE, PARAMETER_NAME, DATA_TYPE, CHARACTER_MAXIMUM_LENGTH, NUMERIC_PRECISION, DEFAULT FROM QSYS2.SYSPARMS WHERE SPECIFIC_SCHEMA IN ('QSYS2','SYSTOOLS') AND (SPECIFIC_NAME LIKE '%SHARE%' OR SPECIFIC_NAME LIKE '%NETSERVER%' OR SPECIFIC_NAME LIKE '%SERVER_SESSION%') ORDER BY 1,2");
        q("QSYS2 views about shares/netserver", "SELECT TABLE_SCHEMA, TABLE_NAME, TABLE_TYPE FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA IN ('QSYS2','SYSTOOLS','SYSIBMADM') AND (TABLE_NAME LIKE '%SHARE%' OR TABLE_NAME LIKE '%NETSERVER%' OR TABLE_NAME LIKE '%SERVER_SESSION%' OR TABLE_NAME LIKE '%NETSVR%')");
        q("columns of those views", "SELECT TABLE_NAME, ORDINAL_POSITION, COLUMN_NAME, SYSTEM_COLUMN_NAME, DATA_TYPE, LENGTH, NUMERIC_SCALE, IS_NULLABLE FROM QSYS2.SYSCOLUMNS WHERE TABLE_SCHEMA IN ('QSYS2','SYSTOOLS') AND (TABLE_NAME LIKE '%SHARE%' OR TABLE_NAME LIKE '%NETSERVER%' OR TABLE_NAME LIKE '%SERVER_SESSION%') ORDER BY 1,2");
        q("SERVER_SHARE_INFO", "SELECT * FROM QSYS2.SERVER_SHARE_INFO");
        q("USER_INFO netserver columns", "SELECT COLUMN_NAME, DATA_TYPE, LENGTH FROM QSYS2.SYSCOLUMNS WHERE TABLE_SCHEMA='QSYS2' AND TABLE_NAME='USER_INFO' AND (COLUMN_NAME LIKE '%NETSERVER%' OR COLUMN_NAME LIKE '%NET%')");
        q("NETSTAT 445/139", "SELECT * FROM QSYS2.NETSTAT_INFO WHERE LOCAL_PORT IN (137,138,139,445)");
        q("jobs QZLS*", "SELECT JOB_NAME, SUBSYSTEM, JOB_TYPE, FUNCTION_TYPE, FUNCTION, JOB_STATUS FROM TABLE(QSYS2.ACTIVE_JOB_INFO(JOB_NAME_FILTER => 'QZLS*')) X");
        q("prestart entries QSERVER", "SELECT * FROM QSYS2.SUBSYSTEM_INFO WHERE SUBSYSTEM_DESCRIPTION='QSERVER'");
        q("prestart job entries", "SELECT * FROM QSYS2.PRESTART_JOB_INFO WHERE PROGRAM_NAME LIKE 'QZLS%'");
        q("autostart", "SELECT * FROM QSYS2.TCPIP_SERVER_INFO WHERE SERVER_NAME LIKE '%NETSVR%'");
        q("function usage", "SELECT FUNCTION_ID, FUNCTION_CATEGORY, FUNCTION_TYPE, DEFAULT_USAGE, ALLOBJ_INDICATOR FROM QSYS2.FUNCTION_INFO WHERE FUNCTION_ID LIKE '%ZLS%' OR FUNCTION_ID LIKE '%NETSERVER%' OR FUNCTION_ID LIKE '%NETSVR%' OR FUNCTION_ID LIKE '%QZLS%'");
        q("exit points", "SELECT EXIT_POINT_NAME, EXIT_POINT_FORMAT, REGISTERED_EXIT_POINT, TEXT_DESCRIPTION FROM QSYS2.EXIT_POINT_INFO WHERE EXIT_POINT_NAME LIKE '%ZLS%' OR EXIT_POINT_NAME LIKE '%QPWFS%' OR EXIT_POINT_NAME LIKE '%NTSV%' OR EXIT_POINT_NAME LIKE '%QNPS%'");
        q("system values pwdlvl", "SELECT SYSTEM_VALUE_NAME, CURRENT_NUMERIC_VALUE, CURRENT_CHARACTER_VALUE FROM QSYS2.SYSTEM_VALUE_INFO WHERE SYSTEM_VALUE_NAME IN ('QPWDLVL','QMAXSIGN','QMAXSGNACN','QSECURITY')");
        q("QSYS2 routines with NETSERVER in text", "SELECT ROUTINE_SCHEMA, ROUTINE_NAME, ROUTINE_TYPE FROM QSYS2.SYSROUTINES WHERE UPPER(LONG_COMMENT) LIKE '%NETSERVER%' OR UPPER(ROUTINE_TEXT) LIKE '%NETSERVER%'");

        // Message IDs: retrieve each candidate's existence and severity only.
        System.out.println("== messages (id sev len-of-text)");
        AS400 s = new AS400(host, user, pass);
        MessageFile mf = new MessageFile(s, "/QSYS.LIB/QCPFMSG.MSGF");
        String hex = "0123456789ABCDEF";
        for (String pre : new String[] {"CPIB6", "CPDB6", "CPFB6", "CPIB7", "CPDB7"}) {
            for (int i = 0; i < 16; i++) for (int j = 0; j < 16; j++) {
                String id = pre + hex.charAt(i) + hex.charAt(j);
                try { AS400Message m = mf.getMessage(id); System.out.println("   " + id + " sev=" + m.getSeverity() + " | " + m.getText()); } catch (Exception e) { }
            }
        }
        // NetServer jobs through the job list API.
        System.out.println("== JobList QZLS*");
        try {
            JobList jl = new JobList(s);
            jl.addJobSelectionCriteria(JobList.SELECTION_JOB_NAME, "QZLS*");
            Enumeration<?> en = jl.getJobs();
            while (en.hasMoreElements()) { Job j = (Job) en.nextElement(); System.out.println("   " + j.getNumber() + "/" + j.getUser() + "/" + j.getName() + " " + j.getStatus() + " " + j.getSubsystem() + " type=" + j.getType() + j.getSubtype()); }
        } catch (Exception e) { System.out.println("   ERR " + e); }
        s.disconnectAllServices();
        c.close();
    }
}
