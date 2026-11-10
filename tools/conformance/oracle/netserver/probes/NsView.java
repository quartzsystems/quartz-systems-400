// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import java.sql.*;

/** Read-only: QSYS2.SERVER_SHARE_INFO's system name and column CCSIDs on pub400. */
public final class NsView {
    public static void main(String[] a) throws Exception {
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        try (Connection c = DriverManager.getConnection("jdbc:as400://" + System.getenv("QS_PUB400_HOST") + ";naming=sql;prompt=false",
                System.getenv("QS_PUB400_USER"), System.getenv("QS_PUB400_PASS"));
             Statement s = c.createStatement()) {
            try (ResultSet r = s.executeQuery("SELECT SYSTEM_TABLE_NAME, TABLE_TYPE FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA='QSYS2' AND TABLE_NAME='SERVER_SHARE_INFO'")) {
                while (r.next()) System.out.println("V QSYS2 SERVER_SHARE_INFO " + r.getString(1).trim() + " " + r.getString(2));
            }
            try (ResultSet r = s.executeQuery("SELECT COLUMN_NAME, SYSTEM_COLUMN_NAME, DATA_TYPE, LENGTH, COALESCE(NUMERIC_SCALE,0), IS_NULLABLE, COALESCE(CCSID,0) FROM QSYS2.SYSCOLUMNS WHERE TABLE_SCHEMA='QSYS2' AND TABLE_NAME='SERVER_SHARE_INFO' ORDER BY ORDINAL_POSITION")) {
                while (r.next()) System.out.println("c " + r.getString(1).trim() + " " + r.getString(2).trim() + " " + r.getString(3).trim() + " " + r.getInt(4) + " " + r.getInt(5) + " " + r.getString(6).trim() + " " + r.getInt(7));
            }
        }
    }
}
