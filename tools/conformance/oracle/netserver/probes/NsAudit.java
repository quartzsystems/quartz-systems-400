// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import java.sql.*;

/** Read-only: the fields of IBM's audit model files named in the args (QSYS). */
public final class NsAudit {
    public static void main(String[] a) throws Exception {
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        String url = "jdbc:as400://" + System.getenv("QS_ORACLE_HOST") + ";naming=system;errors=full";
        try (Connection c = DriverManager.getConnection(url, System.getenv("QS_ORACLE_USER"), System.getenv("QS_ORACLE_PASS"))) {
            for (String f : a) {
                try (PreparedStatement s = c.prepareStatement(
                        "SELECT COLUMN_NAME, ORDINAL_POSITION, DATA_TYPE, LENGTH, NUMERIC_SCALE FROM QSYS2/SYSCOLUMNS"
                                + " WHERE TABLE_SCHEMA = 'QSYS' AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION")) {
                    s.setString(1, f);
                    try (ResultSet r = s.executeQuery()) {
                        while (r.next()) {
                            System.out.println(f + "|" + r.getString(1).trim() + "|" + r.getInt(2) + "|" + r.getString(3).trim()
                                    + "|" + r.getInt(4) + "|" + r.getObject(5));
                        }
                    }
                }
            }
        }
    }
}
