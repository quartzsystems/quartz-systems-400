// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;
import com.ibm.as400.access.IFSFile;
import com.ibm.as400.access.IFSFileInputStream;
import com.ibm.as400.access.IFSFileOutputStream;

/** File server: list, write, read, rename and delete under /home/QSTEST only. */
public final class Ifs {
    public static void main(String[] a) {
        AS400 s = Hs.system();
        Hs.step("connect", () -> s.connectService(AS400.FILE));
        String dir = "/home/QSTEST/hstrace";
        Hs.step("list /home/QSTEST", () -> {
            String[] names = new IFSFile(s, "/home/QSTEST").list();
            Hs.out("     entries=" + (names == null ? "none" : names.length));
        });
        Hs.step("mkdir", () -> Hs.out("     made=" + new IFSFile(s, dir).mkdir()));
        Hs.step("write", () -> {
            try (IFSFileOutputStream o = new IFSFileOutputStream(s, dir + "/a.txt")) {
                o.write("QS/400 host server trace\n".getBytes(java.nio.charset.StandardCharsets.US_ASCII));
            }
        });
        Hs.step("read", () -> {
            try (IFSFileInputStream i = new IFSFileInputStream(s, dir + "/a.txt")) {
                Hs.out("     bytes=" + i.readAllBytes().length);
            }
        });
        Hs.step("attributes", () -> {
            IFSFile f = new IFSFile(s, dir + "/a.txt");
            Hs.out("     length=" + f.length() + " file=" + f.isFile() + " ccsid=" + f.getCCSID());
        });
        Hs.step("rename", () -> Hs.out("     renamed=" + new IFSFile(s, dir + "/a.txt").renameTo(new IFSFile(s, dir + "/b.txt"))));
        Hs.step("delete", () -> {
            Hs.out("     file=" + new IFSFile(s, dir + "/b.txt").delete());
            Hs.out("     dir=" + new IFSFile(s, dir).delete());
        });
        s.disconnectAllServices();
    }
}
