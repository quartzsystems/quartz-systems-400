// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

import com.ibm.as400.access.AS400;
import com.ibm.as400.access.DataQueue;
import com.ibm.as400.access.DataQueueEntry;
import com.ibm.as400.access.KeyedDataQueue;

/** Data queue server: a plain and a keyed queue in QSTEST, written, peeked, read, cleared, deleted. */
public final class DtaQ {
    public static void main(String[] a) {
        AS400 s = Hs.system();
        Hs.step("connect", () -> s.connectService(AS400.DATAQUEUE));
        DataQueue q = new DataQueue(s, "/QSYS.LIB/QSTEST.LIB/HSTRACEQ.DTAQ");
        Hs.step("create", () -> q.create(256));
        Hs.step("write", () -> {
            q.write("first entry");
            q.write("second entry");
        });
        Hs.step("attributes", () -> Hs.out("     max=" + q.getMaxEntryLength() + " fifo=" + q.isFIFO()));
        Hs.step("peek", () -> show(q.peek()));
        Hs.step("read", () -> show(q.read(0)));
        Hs.step("clear", q::clear);
        Hs.step("delete", q::delete);
        KeyedDataQueue k = new KeyedDataQueue(s, "/QSYS.LIB/QSTEST.LIB/HSTRACEK.DTAQ");
        Hs.step("keyed create", () -> k.create(4, 128));
        Hs.step("keyed write", () -> k.write("K001", "keyed entry"));
        Hs.step("keyed read", () -> show(k.read("K001", 0, "EQ")));
        Hs.step("keyed delete", k::delete);
        s.disconnectAllServices();
    }

    static void show(DataQueueEntry e) throws Exception {
        Hs.out("     entry=" + (e == null ? "none" : e.getString()));
    }
}
