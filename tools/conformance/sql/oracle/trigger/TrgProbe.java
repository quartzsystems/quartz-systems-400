// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;
import com.ibm.as400.access.DataQueue;
import com.ibm.as400.access.AS400JDBCConnection;
import com.ibm.as400.access.DataQueueEntry;
import com.ibm.as400.access.JobLog;
import com.ibm.as400.access.QueuedMessage;
import java.nio.file.Files;
import java.nio.file.Path;
import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.ResultSet;
import java.sql.ResultSetMetaData;
import java.sql.SQLException;
import java.sql.SQLWarning;
import java.sql.Statement;
import java.util.ArrayList;
import java.util.List;

/**
 * The trigger probe: runs a script of SQL statements (one per line ending in ';') on the
 * primary over jdbc:as400, as SqlProbe does, and at a line "-- dtaq NAME" prints every entry
 * of data queue QSTEST/NAME in hex, read and removed (a CL trigger program sends its
 * trigger buffer there), and at a line "-- cmd COMMAND" runs the CL command through QCMDEXC, and at "-- joblog" prints the
 * server job's messages since the last look. Usage: TrgProbe FILE. Credentials from QS_ORACLE_*.
 */
public final class TrgProbe {
    private static String user = "";

    private static String env(String k) {
        String v = System.getenv(k);
        if (v == null || v.isEmpty()) {
            throw new IllegalStateException(k + " is not set");
        }
        return v;
    }

    private static String mask(String s) {
        if (s == null) {
            return "-";
        }
        return s.replace(user.toUpperCase(), "{user}").replace(user.toLowerCase(), "{user}");
    }

    private static String msgid(String text) {
        if (text != null && text.startsWith("[") && text.indexOf(']') > 0) {
            return text.substring(1, text.indexOf(']'));
        }
        return "-";
    }

    private static String hex(byte[] b, int n) {
        StringBuilder s = new StringBuilder();
        for (int i = 0; i < n; i++) {
            if (i % 32 == 0) {
                s.append(String.format("%n    %04d ", i));
            }
            s.append(String.format("%02X", b[i]));
        }
        return s.toString();
    }

    private static void run(Connection c, String sql) {
        System.out.println("> " + mask(sql.replace('\n', ' ')));
        try (Statement s = c.createStatement()) {
            boolean rs = s.execute(sql);
            for (SQLWarning w = s.getWarnings(); w != null; w = w.getNextWarning()) {
                System.out.println("  warning " + w.getErrorCode() + " " + w.getSQLState() + " " + msgid(w.getMessage()) + " " + mask(w.getMessage()));
            }
            if (!rs) {
                System.out.println("  sqlcode 0 count " + s.getUpdateCount());
                return;
            }
            try (ResultSet r = s.getResultSet()) {
                ResultSetMetaData m = r.getMetaData();
                StringBuilder h = new StringBuilder("  cols");
                for (int i = 1; i <= m.getColumnCount(); i++) {
                    h.append(" | ").append(m.getColumnName(i)).append(' ').append(m.getColumnTypeName(i))
                        .append('(').append(m.getPrecision(i)).append(',').append(m.getScale(i)).append(')')
                        .append(m.isNullable(i) == ResultSetMetaData.columnNoNulls ? " NN" : "");
                }
                System.out.println(h);
                while (r.next()) {
                    StringBuilder row = new StringBuilder("  row");
                    for (int i = 1; i <= m.getColumnCount(); i++) {
                        String x = r.getString(i);
                        row.append(" | ").append(mask(x == null ? "-" : "'" + x + "'"));
                    }
                    System.out.println(row);
                }
            }
        } catch (SQLException e) {
            System.out.println("  sqlcode " + e.getErrorCode() + " " + e.getSQLState() + " " + msgid(e.getMessage()) + " " + mask(e.getMessage()));
        }
    }

    private static void dtaq(AS400 sys, String name) throws Exception {
        DataQueue q = new DataQueue(sys, "/QSYS.LIB/QSTEST.LIB/" + name + ".DTAQ");
        int n = 0;
        for (DataQueueEntry e = q.read(0); e != null; e = q.read(0)) {
            byte[] b = e.getData();
            System.out.println("  entry " + (++n) + " length " + b.length + hex(b, b.length));
        }
        System.out.println("  entries " + n);
    }

    private static int seen = 0;

    /** The connection's server job's messages since the last look: ID, type, first-level text. */
    private static void joblog(AS400 sys, Connection c) throws Exception {
        String id = ((AS400JDBCConnection) c).getServerJobIdentifier();
        JobLog log = new JobLog(sys, id.substring(0, 10).trim(), id.substring(10, 20).trim(), id.substring(20).trim());
        QueuedMessage[] all = log.getMessages(-1, 0);
        for (int i = seen; i < all.length; i++) {
            QueuedMessage m = all[i];
            System.out.println("  msg " + m.getID() + " type " + m.getType() + " sev " + m.getSeverity() + " " + mask(m.getText()));
        }
        seen = all.length;
    }

    public static void main(String[] a) throws Exception {
        user = env("QS_ORACLE_USER");
        String host = env("QS_ORACLE_HOST");
        String pass = env("QS_ORACLE_PASS");
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        String url = "jdbc:as400://" + host + ";naming=sql;prompt=false;errors=full;libraries=QSTEST";
        AS400 sys = new AS400(host, user, pass);
        try (Connection c = DriverManager.getConnection(url, user, pass)) {
            c.setAutoCommit(true);
            run(c, "SET SCHEMA QSTEST");
            StringBuilder cur = new StringBuilder();
            for (String line : Files.readAllLines(Path.of(a[0]))) {
                String t = line.trim();
                if (t.startsWith("-- dtaq ")) {
                    System.out.println("> " + t);
                    dtaq(sys, t.substring(8).trim());
                    continue;
                }
                if (t.equals("-- joblog")) {
                    System.out.println("> -- joblog");
                    joblog(sys, c);
                    continue;
                }
                if (t.startsWith("-- cmd ")) {
                    String cmd = t.substring(7).trim();
                    run(c, "CALL QSYS.QCMDEXC('" + cmd.replace("'", "''") + "', " + String.format("%010d.00000", cmd.length()) + ")");
                    continue;
                }
                if (t.startsWith("--") && cur.length() == 0) {
                    continue;
                }
                if (t.endsWith(";")) {
                    cur.append(line, 0, line.lastIndexOf(';'));
                    String s = cur.toString().trim();
                    cur.setLength(0);
                    if (!s.isEmpty()) {
                        run(c, s);
                    }
                } else {
                    cur.append(line).append('\n');
                }
            }
        }
        sys.disconnectAllServices();
    }
}
