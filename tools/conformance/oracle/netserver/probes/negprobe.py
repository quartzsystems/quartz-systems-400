# Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
# licensed under the terms of the MIT license which can be found in the
# root directory of this project.

"""SMB negotiate-only probe: no session setup, nothing changed on the server."""
import os, socket, struct, sys, uuid

host = os.environ["QS_ORACLE_HOST"]

def nb(payload):
    return struct.pack(">I", len(payload)) + payload

def recv_msg(s):
    hdr = b""
    while len(hdr) < 4:
        c = s.recv(4 - len(hdr))
        if not c: return None
        hdr += c
    n = struct.unpack(">I", hdr)[0] & 0xFFFFFF
    data = b""
    while len(data) < n:
        c = s.recv(n - len(data))
        if not c: break
        data += c
    return data

def smb2_negotiate(dialects):
    h = b"\xfeSMB" + struct.pack("<HHIHHIIQIIQ16s", 64, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, b"\0" * 16)
    body = struct.pack("<HHHHI16sQ", 36, len(dialects), 1, 0, 0x7F, uuid.uuid4().bytes, 0)
    body += b"".join(struct.pack("<H", d) for d in dialects)
    return h + body

def smb1_negotiate():
    dial = b"\x02NT LM 0.12\0\x02SMB 2.002\0\x02SMB 2.???\0"
    h = b"\xffSMB" + bytes([0x72]) + b"\0" * 4 + bytes([0x18]) + struct.pack("<H", 0xC853) + b"\0" * 12 + struct.pack("<HHHH", 0, 0xFFFE, 0, 0)
    return h + b"\0" + struct.pack("<H", len(dial)) + dial

def show(tag, r):
    if r is None:
        print(tag, "closed"); return
    print(tag, "len", len(r), "proto", r[:4])
    if r[:4] == b"\xfeSMB":
        status = struct.unpack_from("<I", r, 8)[0]
        b = r[64:]
        sm, dialect = struct.unpack_from("<HH", b, 2)
        caps, maxt, maxr, maxw = struct.unpack_from("<IIII", b, 24)
        sbo, sbl = struct.unpack_from("<HH", b, 56)
        print(f"  status={status:#x} secmode={sm:#x} dialect={dialect:#06x} caps={caps:#x} maxtrans={maxt} maxread={maxr} maxwrite={maxw} secblob={sbl}")
        print("  guid", b[8:24].hex(), "systime", struct.unpack_from("<Q", b, 40)[0])
        print("  blob", r[sbo:sbo+sbl].hex())
    else:
        print("  raw", r[:80].hex())

for tag, dialects in [("smb2 all", [0x202, 0x210, 0x300, 0x302, 0x311]), ("smb2 202", [0x202]), ("smb2 3x", [0x300, 0x302])]:
    s = socket.create_connection((host, 445), timeout=10)
    msg = smb2_negotiate(dialects)
    s.sendall(nb(msg))
    show(tag, recv_msg(s))
    s.close()

s = socket.create_connection((host, 445), timeout=10)
s.sendall(nb(smb1_negotiate()))
show("smb1 multi", recv_msg(s))
s.close()
