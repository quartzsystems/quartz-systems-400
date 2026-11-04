// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;
import com.ibm.as400.access.PrintObjectInputStream;
import com.ibm.as400.access.SpooledFile;
import com.ibm.as400.access.SpooledFileList;

/** Network print server: read the bytes of the user's smallest spooled file (read only). */
public final class SplfData {
    public static void main(String[] a) {
        AS400 s = Hs.system();
        Hs.step("connect", () -> s.connectService(AS400.PRINT));
        Hs.step("read", () -> {
            SpooledFileList l = new SpooledFileList(s);
            l.setUserFilter("*CURRENT");
            l.openSynchronously();
            SpooledFile pick = null;
            int pages = Integer.MAX_VALUE;
            for (int i = 0; i < l.size(); i++) {
                SpooledFile f = (SpooledFile) l.getObject(i);
                Integer p = f.getIntegerAttribute(SpooledFile.ATTR_PAGES);
                if (p != null && p > 0 && p < pages) {
                    pages = p;
                    pick = f;
                }
            }
            l.close();
            if (pick == null) {
                Hs.out("     no spooled file");
                return;
            }
            Hs.out("     file=" + pick.getName() + " pages=" + pages);
            long n = 0;
            int first = -1;
            try (PrintObjectInputStream in = pick.getInputStream(null)) {
                byte[] buf = new byte[4096];
                int r;
                while ((r = in.read(buf)) > 0) {
                    if (first < 0) {
                        first = buf[0] & 0xFF;
                    }
                    n += r;
                }
            }
            Hs.out("     bytes>0=" + (n > 0) + " first=" + String.format("%02X", first));
        });
        s.disconnectAllServices();
    }
}
