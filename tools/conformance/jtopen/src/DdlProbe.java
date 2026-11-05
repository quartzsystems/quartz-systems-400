// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

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
import java.util.Properties;

/**
 * Runs an SQL script in the conformance kit's format (statements end with ';' at end of
 * line, '--#SET TERMINATOR' switches the terminator, '{lib}' is QS_SQL_LIB) on one
 * connection with *SQL naming and autocommit, and prints for each statement its SQLCODE,
 * SQLSTATE, message ID, row count and result set. A probe for oracle facts until the kit
 * runner lands; the user's name is printed as {user}.
 */
public final class DdlProbe {
    public static void main(String[] a) throws Exception {
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        String lib = System.getenv().getOrDefault("QS_SQL_LIB", "QSTEST");
        String user = Hs.user();
        Properties p = new Properties();
        p.setProperty("naming", "sql");
        p.setProperty("prompt", "false");
        p.setProperty("errors", "full");
        p.setProperty("libraries", lib);
        p.setProperty("user", user);
        p.setProperty("password", Hs.pass());
        String url = "jdbc:as400://" + Hs.host() + (Hs.skipSignon() ? ":8471" : "");
        try (Connection c = Hs.skipSignon()
                ? new com.ibm.as400.access.AS400JDBCDriver().connect(Hs.system(), p, null)
                : DriverManager.getConnection(url, p)) {
            try (Statement s = c.createStatement()) {
                s.execute("SET SCHEMA " + lib);
            }
            for (String file : a) {
                System.out.println("== " + Path.of(file).getFileName());
                for (String stmt : split(Files.readString(Path.of(file)))) {
                    run(c, stmt.replace("{lib}", lib), user, lib);
                }
            }
        }
    }

    private static List<String> split(String text) {
        List<String> out = new ArrayList<>();
        String term = ";";
        StringBuilder cur = new StringBuilder();
        for (String line : text.split("\r?\n")) {
            String t = line.trim();
            if (t.toUpperCase().startsWith("--#SET TERMINATOR")) {
                term = t.substring("--#SET TERMINATOR".length()).trim();
                continue;
            }
            if (t.startsWith("--") && cur.toString().isBlank()) {
                continue;
            }
            if (t.endsWith(term)) {
                cur.append(line, 0, line.lastIndexOf(term));
                if (!cur.toString().isBlank()) {
                    out.add(cur.toString().trim());
                }
                cur.setLength(0);
            } else {
                cur.append(line).append('\n');
            }
        }
        if (!cur.toString().isBlank()) {
            out.add(cur.toString().trim());
        }
        return out;
    }

    /** "!AUT LIB/OBJ.TYPE": the object's authorities as DSPOBJAUT shows them. */
    private static void authorities(String obj, String user, String lib) {
        try {
            String[] lo = obj.split("/");
            String path = "/QSYS.LIB/" + lo[0] + ".LIB/" + lo[1];
            com.ibm.as400.access.Permission p = new com.ibm.as400.access.Permission(Hs.system(), path);
            List<String> lines = new ArrayList<>();
            java.util.Enumeration<?> e = p.getUserPermissions();
            while (e.hasMoreElements()) {
                com.ibm.as400.access.QSYSPermission u = (com.ibm.as400.access.QSYSPermission) e.nextElement();
                lines.add("  aut " + u.getUserID() + " " + u.getObjectAuthority()
                    + " opr=" + u.isOperational() + " mgt=" + u.isManagement() + " exs=" + u.isExistence()
                    + " alt=" + u.isAlter() + " ref=" + u.isReference() + " read=" + u.isRead()
                    + " add=" + u.isAdd() + " upd=" + u.isUpdate() + " dlt=" + u.isDelete()
                    + " exe=" + u.isExecute() + " autl=" + u.isAuthorizationListManagement());
            }
            java.util.Collections.sort(lines);
            for (String l : lines) {
                System.out.println(mask(l, user, lib));
            }
        } catch (Exception ex) {
            System.out.println("  error " + mask(String.valueOf(ex), user, lib));
        }
    }

    private static String mask(String s, String user, String lib) {
        if (s == null) {
            return "-";
        }
        String r = s.replace(user.toUpperCase(), "{user}").replace(user.toLowerCase(), "{user}");
        return lib.equals("QSTEST") ? r : r.replace(lib, "{lib}");
    }

    private static void run(Connection c, String sql, String user, String lib) {
        System.out.println("> " + mask(sql.replace('\n', ' '), user, lib));
        if (sql.startsWith("!AUT ")) {
            authorities(sql.substring(5).trim(), user, lib);
            return;
        }
        if (sql.startsWith("!CMD ")) {
            try {
                com.ibm.as400.access.CommandCall cc = new com.ibm.as400.access.CommandCall(Hs.system());
                System.out.println("  cmd " + cc.run(sql.substring(5).trim()));
                for (com.ibm.as400.access.AS400Message m : cc.getMessageList()) {
                    System.out.println("  msg " + m.getID() + " " + mask(m.getText(), user, lib));
                }
            } catch (Exception ex) {
                System.out.println("  error " + mask(String.valueOf(ex), user, lib));
            }
            return;
        }
        try (Statement s = c.createStatement()) {
            boolean rs = s.execute(sql);
            SQLWarning w = s.getWarnings();
            if (w != null) {
                System.out.println("  warning " + w.getErrorCode() + " " + w.getSQLState() + " " + mask(w.getMessage(), user, lib));
            }
            if (!rs) {
                System.out.println("  ok count=" + s.getUpdateCount());
                return;
            }
            try (ResultSet r = s.getResultSet()) {
                ResultSetMetaData m = r.getMetaData();
                StringBuilder h = new StringBuilder("  cols");
                for (int i = 1; i <= m.getColumnCount(); i++) {
                    h.append(" | ").append(m.getColumnName(i)).append(' ').append(m.getColumnTypeName(i))
                        .append('(').append(m.getPrecision(i)).append(',').append(m.getScale(i)).append(')')
                        .append(m.isNullable(i) == ResultSetMetaData.columnNullable ? " null" : "");
                }
                System.out.println(mask(h.toString(), user, lib));
                int n = 0;
                while (r.next() && n < 200) {
                    StringBuilder row = new StringBuilder("  row");
                    for (int i = 1; i <= m.getColumnCount(); i++) {
                        String v = r.getString(i);
                        row.append(" | ").append(v == null ? "-" : v.stripTrailing());
                    }
                    System.out.println(mask(row.toString(), user, lib));
                    n++;
                }
                System.out.println("  rows=" + n);
            }
        } catch (SQLException e) {
            String msg = e.getMessage();
            String id = msg != null && msg.startsWith("[") ? msg.substring(1, msg.indexOf(']')) : "-";
            System.out.println("  error " + e.getErrorCode() + " " + e.getSQLState() + " " + id + " " + mask(msg, user, lib));
        }
    }
}
