// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import java.sql.Connection;
import java.sql.DriverManager;
import java.sql.PreparedStatement;
import java.sql.ResultSet;
import java.sql.SQLException;

/**
 * The 256-byte sweep of single-byte CCSIDs on the primary oracle over jdbc:as400: every
 * byte 00-FF tagged with the CCSID (a FOR BIT DATA value cast to it, no conversion),
 * converted to UTF-16 (1200) and back. Prints, per CCSID, `ccsid N` and either `sqlcode`
 * or the 256 UTF-16 units and the round trip's 256 bytes, in hex, 16 per line. Then which
 * Unicode CCSIDs the system converts 'A' (CCSID 37) to (`uN`), and bidi CCSIDs a byte at a
 * time (`pN`). Usage: CcsidProbe CCSID|uN|pN...
 * Credentials come from QS_ORACLE_*.
 */
public final class CcsidProbe {
    private static String env(String k) {
        String v = System.getenv(k);
        if (v == null || v.isEmpty()) {
            throw new IllegalStateException(k + " is not set");
        }
        return v;
    }

    private static String msgid(String text) {
        if (text != null && text.startsWith("[") && text.indexOf(']') > 0) {
            return text.substring(1, text.indexOf(']'));
        }
        return "-";
    }

    private static void lines(String label, String hex, int per) {
        for (int i = 0; i < hex.length(); i += per) {
            System.out.println("  " + label + " " + hex.substring(i, Math.min(hex.length(), i + per)));
        }
    }

    private static void sweep(Connection c, int ccsid) {
        System.out.println("ccsid " + ccsid);
        byte[] all = new byte[256];
        for (int i = 0; i < 256; i++) {
            all[i] = (byte) i;
        }
        String tagged = "CAST(CAST(? AS VARCHAR(256) FOR BIT DATA) AS VARCHAR(256) CCSID " + ccsid + ")";
        String u = "CAST(" + tagged + " AS VARGRAPHIC(256) CCSID 1200)";
        String back = "CAST(CAST(" + u + " AS VARCHAR(256) CCSID " + ccsid + ") AS VARCHAR(256) FOR BIT DATA)";
        String sql = "SELECT HEX(" + u + "), HEX(" + back + ") FROM SYSIBM.SYSDUMMY1";
        try (PreparedStatement p = c.prepareStatement(sql)) {
            p.setBytes(1, all);
            p.setBytes(2, all);
            try (ResultSet r = p.executeQuery()) {
                r.next();
                lines("u", r.getString(1), 64);
                lines("b", r.getString(2), 32);
            }
        } catch (SQLException e) {
            System.out.println("  sqlcode " + e.getErrorCode() + " " + e.getSQLState() + " " + msgid(e.getMessage()));
        }
    }

    /** Each byte alone (bidi CCSIDs reorder and reshape whole strings): `p` lines, byte and UTF-16. */
    private static void perByte(Connection c, int ccsid) {
        System.out.println("ccsid " + ccsid + " per byte");
        StringBuilder v = new StringBuilder();
        for (int i = 0; i < 256; i++) {
            v.append(i == 0 ? "" : ",").append(String.format("(X'%02X')", i));
        }
        String tagged = "CAST(CAST(B AS VARCHAR(1) FOR BIT DATA) AS VARCHAR(1) CCSID " + ccsid + ")";
        String u = "CAST(" + tagged + " AS VARGRAPHIC(4) CCSID 1200)";
        String back = "CAST(CAST(" + u + " AS VARCHAR(4) CCSID " + ccsid + ") AS VARCHAR(4) FOR BIT DATA)";
        String sql = "WITH T(B) AS (VALUES " + v + ") SELECT HEX(B), HEX(" + u + "), HEX(" + back + ") FROM T ORDER BY B";
        try (PreparedStatement p = c.prepareStatement(sql); ResultSet r = p.executeQuery()) {
            while (r.next()) {
                System.out.println("  p " + r.getString(1) + " " + r.getString(2) + " " + r.getString(3));
            }
        } catch (SQLException e) {
            System.out.println("  sqlcode " + e.getErrorCode() + " " + e.getSQLState() + " " + msgid(e.getMessage()));
        }
    }

    private static void unicode(Connection c, int ccsid) {
        String type = ccsid == 1208 ? "VARCHAR" : "VARGRAPHIC";
        String sql = "SELECT HEX(CAST(CAST('A' AS VARCHAR(4) CCSID 37) AS " + type + "(4) CCSID " + ccsid
            + ")) FROM SYSIBM.SYSDUMMY1";
        try (PreparedStatement p = c.prepareStatement(sql); ResultSet r = p.executeQuery()) {
            r.next();
            System.out.println("unicode " + ccsid + " " + type + " " + r.getString(1));
        } catch (SQLException e) {
            System.out.println("unicode " + ccsid + " " + type + " sqlcode " + e.getErrorCode() + " " + e.getSQLState() + " "
                + msgid(e.getMessage()));
        }
    }

    public static void main(String[] a) throws Exception {
        Class.forName("com.ibm.as400.access.AS400JDBCDriver");
        String url = "jdbc:as400://" + env("QS_ORACLE_HOST") + ";naming=sql;prompt=false;errors=full;translate binary=false";
        try (Connection c = DriverManager.getConnection(url, env("QS_ORACLE_USER"), env("QS_ORACLE_PASS"))) {
            for (String s : a) {
                if (s.startsWith("p")) {
                    perByte(c, Integer.parseInt(s.substring(1)));
                } else if (s.startsWith("u")) {
                    unicode(c, Integer.parseInt(s.substring(1)));
                } else {
                    sweep(c, Integer.parseInt(s));
                }
            }
        }
    }
}
