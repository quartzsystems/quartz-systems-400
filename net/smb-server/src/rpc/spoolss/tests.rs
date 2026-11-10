//! The spoolss calls against a print service in memory: requests built
//! as a client marshals them, responses decoded field by field.

use super::mem::MemPrint;
use super::*;
use crate::rpc::pdu::NCA_OP_RNG_ERROR;

const SERVER: &str = "QSHQ400";

fn alice() -> Identity {
    Identity::User {
        user: "ALICE".into(),
        domain: String::new(),
    }
}

struct Fixture {
    print: MemPrint,
    who: Identity,
    session: Session,
}

impl Fixture {
    fn new() -> Self {
        Self {
            print: MemPrint::default(),
            who: alice(),
            session: Session::default(),
        }
    }

    async fn call(&mut self, opnum: u16, stub: &[u8]) -> Result<Vec<u8>, Fault> {
        let ctx = Ctx {
            service: &self.print,
            server: SERVER,
            who: &self.who,
        };
        call(&ctx, &mut self.session, opnum, stub).await
    }

    async fn ok(&mut self, opnum: u16, stub: &[u8]) -> Vec<u8> {
        self.call(opnum, stub).await.expect("no fault")
    }

    /// RpcOpenPrinterEx of `name`; the handle and the status.
    async fn open(&mut self, name: &str, access: u32) -> ([u8; 20], u32) {
        let stub = open_request(Some(name), None, access);
        let out = self.ok(opnum::OPEN_PRINTER_EX, &stub).await;
        let mut r = NdrReader::new(&out);
        let handle = r.handle().unwrap();
        let status = r.u32().unwrap();
        assert!(r.at_end());
        (handle, status)
    }

    async fn printer(&mut self, name: &str) -> [u8; 20] {
        let (handle, status) = self.open(name, 0x8).await;
        assert_eq!(status, 0, "{name}");
        handle
    }
}

fn open_request(name: Option<&str>, datatype: Option<&str>, access: u32) -> Vec<u8> {
    let mut w = NdrWriter::new();
    w.pointer(name.is_some());
    if let Some(name) = name {
        w.string(name);
    }
    w.pointer(datatype.is_some());
    if let Some(datatype) = datatype {
        w.string(datatype);
    }
    // An empty DEVMODE_CONTAINER, the access, SPLCLIENT_INFO_1.
    w.u32(0);
    w.pointer(false);
    w.u32(access);
    w.u32(1);
    w.u32(1);
    w.pointer(true);
    w.u32(28);
    w.pointer(true);
    w.pointer(true);
    w.u32(19045);
    w.u32(10);
    w.u32(0);
    w.u32(9);
    w.string("\\\\DESKTOP-1");
    w.string("alice");
    w.finish()
}

/// `handle`, then `rest`.
fn with_handle(handle: &[u8; 20], rest: impl FnOnce(&mut NdrWriter)) -> Vec<u8> {
    let mut w = NdrWriter::new();
    w.handle(handle);
    rest(&mut w);
    w.finish()
}

/// An INFO buffer of `len` bytes, or none, and cbBuf.
fn buffer(w: &mut NdrWriter, len: Option<u32>) {
    w.pointer(len.is_some());
    if let Some(len) = len {
        w.bytes(&vec![0; len as usize]);
    }
    w.u32(len.unwrap_or(0));
}

/// An INFO answer: the buffer (when sent), pcbNeeded, the counts after it
/// and the status.
struct Info {
    buf: Option<Vec<u8>>,
    needed: u32,
    rest: Vec<u32>,
}

fn info_answer(out: &[u8], counts: usize) -> Info {
    let mut r = NdrReader::new(out);
    let buf = r
        .pointer()
        .unwrap()
        .then(|| r.bytes(1 << 20).unwrap().to_vec());
    let needed = r.u32().unwrap();
    let rest = (0..=counts).map(|_| r.u32().unwrap()).collect();
    assert!(r.at_end(), "bytes after the stub");
    Info { buf, needed, rest }
}

fn u32_at(buf: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(buf[at..at + 4].try_into().unwrap())
}

fn string_at(buf: &[u8], start: usize, at: usize) -> String {
    let from = start + u32_at(buf, start + at) as usize;
    let units: Vec<u16> = buf[from..]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16(&units).unwrap()
}

/// A call that asks for the size first, then with that much room, as
/// clients do; returns the filled buffer and the counts and status.
async fn sized(
    f: &mut Fixture,
    opnum: u16,
    counts: usize,
    req: impl Fn(Option<u32>) -> Vec<u8>,
) -> (Vec<u8>, Vec<u32>) {
    let first = info_answer(&f.ok(opnum, &req(None)).await, counts);
    assert!(first.buf.is_none());
    assert_eq!(
        *first.rest.last().unwrap(),
        codes::ERROR_INSUFFICIENT_BUFFER
    );
    let small = info_answer(&f.ok(opnum, &req(Some(first.needed - 1))).await, counts);
    assert_eq!(
        small.buf.as_ref().unwrap(),
        &vec![0; first.needed as usize - 1]
    );
    assert_eq!(
        *small.rest.last().unwrap(),
        codes::ERROR_INSUFFICIENT_BUFFER
    );
    let full = info_answer(&f.ok(opnum, &req(Some(first.needed + 8))).await, counts);
    assert_eq!(full.needed, first.needed);
    assert_eq!(*full.rest.last().unwrap(), 0);
    (full.buf.unwrap(), full.rest)
}

#[tokio::test]
async fn open_finds_printers_by_unc_or_share_and_refuses_the_rest() {
    let mut f = Fixture::new();
    for name in ["\\\\QSHQ400\\PRT01", "\\\\10.0.0.5\\laser", "PRT01"] {
        let (handle, status) = f.open(name, 0x8).await;
        assert_eq!(status, 0, "{name}");
        assert_ne!(handle, [0; 20]);
    }
    for name in [
        "\\\\QSHQ400\\NOPE",
        "\\\\QSHQ400\\PRT01,Job 3",
        "\\\\QSHQ400\\,XcvMonitor Local Port",
    ] {
        assert_eq!(
            f.open(name, 0x8).await,
            ([0; 20], codes::ERROR_INVALID_PRINTER_NAME),
            "{name}"
        );
    }
    // PRINTER_ALL_ACCESS and SERVER_ALL_ACCESS.
    assert_eq!(
        f.open("PRT01", 0x000F_000C).await.1,
        codes::ERROR_ACCESS_DENIED
    );
    assert_eq!(
        f.open("\\\\QSHQ400", 0x000F_0003).await.1,
        codes::ERROR_ACCESS_DENIED
    );
    assert_eq!(f.open("\\\\QSHQ400", 0x2).await.1, 0);
    let emf = open_request(Some("PRT01"), Some("NT EMF 1.008"), 0x8);
    let out = f.ok(opnum::OPEN_PRINTER_EX, &emf).await;
    assert_eq!(u32_at(&out, 20), codes::ERROR_INVALID_DATATYPE);
    assert_eq!(f.session.open_handles(), 4);
}

#[tokio::test]
async fn an_anonymous_session_sees_no_printers() {
    let mut f = Fixture::new();
    f.who = Identity::Anonymous;
    assert_eq!(
        f.open("PRT01", 0x8).await.1,
        codes::ERROR_INVALID_PRINTER_NAME
    );
}

#[tokio::test]
async fn close_ends_the_handle() {
    let mut f = Fixture::new();
    let handle = f.printer("PRT01").await;
    let out = f
        .ok(opnum::CLOSE_PRINTER, &with_handle(&handle, |_| {}))
        .await;
    assert_eq!(out, [0; 24]);
    let again = f
        .call(opnum::CLOSE_PRINTER, &with_handle(&handle, |_| {}))
        .await;
    assert_eq!(again.unwrap_err().status, CONTEXT_MISMATCH);
}

#[tokio::test]
async fn get_printer_2_tells_the_driver_and_share() {
    let mut f = Fixture::new();
    let handle = f.printer("\\\\QSHQ400\\PRT01").await;
    let (buf, rest) = sized(&mut f, opnum::GET_PRINTER, 0, |len| {
        with_handle(&handle, |w| {
            w.u32(2);
            buffer(w, len);
        })
    })
    .await;
    assert_eq!(rest, [0]);
    assert_eq!(string_at(&buf, 0, 4), "\\\\QSHQ400\\PRT01");
    assert_eq!(string_at(&buf, 0, 8), "PRT01");
    assert_eq!(string_at(&buf, 0, 16), "Microsoft PCL6 Class Driver");
    for level in [0, 1, 3, 4, 5, 7, 8] {
        let stub = with_handle(&handle, |w| {
            w.u32(level);
            buffer(w, Some(4096));
        });
        let a = info_answer(&f.ok(opnum::GET_PRINTER, &stub).await, 0);
        assert_eq!(a.rest, [0], "level {level}");
    }
    for level in [6, 9] {
        let stub = with_handle(&handle, |w| {
            w.u32(level);
            buffer(w, Some(4096));
        });
        let a = info_answer(&f.ok(opnum::GET_PRINTER, &stub).await, 0);
        assert_eq!((a.needed, a.rest[0]), (0, codes::ERROR_INVALID_LEVEL));
    }
    let server = f.open("\\\\QSHQ400", 0x2).await.0;
    let stub = with_handle(&server, |w| {
        w.u32(2);
        buffer(w, Some(4096));
    });
    let a = info_answer(&f.ok(opnum::GET_PRINTER, &stub).await, 0);
    assert_eq!(a.rest, [codes::ERROR_INVALID_HANDLE]);
}

fn enum_request(flags: u32, name: Option<&str>, level: u32, len: Option<u32>) -> Vec<u8> {
    let mut w = NdrWriter::new();
    w.u32(flags);
    w.pointer(name.is_some());
    if let Some(name) = name {
        w.string(name);
    }
    w.u32(level);
    buffer(&mut w, len);
    w.finish()
}

#[tokio::test]
async fn enum_printers_lists_the_shares_at_each_level() {
    let mut f = Fixture::new();
    for level in [1, 2, 4, 5] {
        let (buf, rest) = sized(&mut f, opnum::ENUM_PRINTERS, 1, |len| {
            enum_request(0x2, Some("\\\\QSHQ400"), level, len)
        })
        .await;
        assert_eq!(rest, [2, 0], "level {level}");
        if level == 1 {
            assert_eq!(string_at(&buf, 0, 8), "\\\\QSHQ400\\PRT01");
            assert_eq!(string_at(&buf, 16, 8), "\\\\QSHQ400\\LASER");
            assert_eq!(
                string_at(&buf, 0, 4),
                "\\\\QSHQ400\\PRT01,Microsoft PCL6 Class Driver,PRT01 text"
            );
        }
    }
    let a = info_answer(
        &f.ok(opnum::ENUM_PRINTERS, &enum_request(0x2, None, 3, None))
            .await,
        1,
    );
    assert_eq!(a.rest, [0, codes::ERROR_INVALID_LEVEL]);
    let a = info_answer(
        &f.ok(
            opnum::ENUM_PRINTERS,
            &enum_request(0x8, Some("\\\\QSHQ400\\PRT01"), 1, None),
        )
        .await,
        1,
    );
    assert_eq!(a.rest, [0, codes::ERROR_INVALID_NAME]);
    // PRINTER_ENUM_CONNECTIONS: none here.
    let a = info_answer(
        &f.ok(opnum::ENUM_PRINTERS, &enum_request(0x4, None, 1, Some(0)))
            .await,
        1,
    );
    assert_eq!((a.needed, a.rest.clone()), (0, vec![0, 0]));
}

fn driver_request(handle: &[u8; 20], env: Option<&str>, level: u32, len: Option<u32>) -> Vec<u8> {
    with_handle(handle, |w| {
        w.pointer(env.is_some());
        if let Some(env) = env {
            w.string(env);
        }
        w.u32(level);
        buffer(w, len);
        w.u32(3);
        w.u32(0);
    })
}

#[tokio::test]
async fn get_printer_driver_2_names_the_class_driver_with_no_files() {
    let mut f = Fixture::new();
    let handle = f.printer("LASER").await;
    for level in [1, 2, 3, 4, 6, 8] {
        let (buf, rest) = sized(&mut f, opnum::GET_PRINTER_DRIVER_2, 2, |len| {
            driver_request(&handle, Some("Windows x64"), level, len)
        })
        .await;
        assert_eq!(rest, [3, 0, 0], "level {level}");
        if level == 1 {
            assert_eq!(string_at(&buf, 0, 0), "Microsoft PS Class Driver");
        } else {
            assert_eq!(u32_at(&buf, 0), 3);
            assert_eq!(string_at(&buf, 0, 4), "Microsoft PS Class Driver");
            assert_eq!(string_at(&buf, 0, 8), "Windows x64");
            assert_eq!(string_at(&buf, 0, 12), "");
        }
    }
    let a = info_answer(
        &f.ok(
            opnum::GET_PRINTER_DRIVER_2,
            &driver_request(&handle, Some("Windows 4.0"), 3, None),
        )
        .await,
        2,
    );
    assert_eq!(a.rest, [0, 0, codes::ERROR_INVALID_ENVIRONMENT]);
    let a = info_answer(
        &f.ok(
            opnum::GET_PRINTER_DRIVER_2,
            &driver_request(&handle, None, 5, None),
        )
        .await,
        2,
    );
    assert_eq!(a.rest[2], codes::ERROR_INVALID_LEVEL);
}

#[tokio::test]
async fn forms_are_listed_and_found_by_name() {
    let mut f = Fixture::new();
    let handle = f.printer("PRT01").await;
    for level in [1, 2] {
        let (buf, rest) = sized(&mut f, opnum::ENUM_FORMS, 1, |len| {
            with_handle(&handle, |w| {
                w.u32(level);
                buffer(w, len);
            })
        })
        .await;
        assert_eq!(rest, [10, 0]);
        assert_eq!(string_at(&buf, 0, 4), "Letter");
    }
    let get = |name: &str, level: u32| {
        with_handle(&handle, |w| {
            w.string(name);
            w.u32(level);
            buffer(w, Some(512));
        })
    };
    let a = info_answer(&f.ok(opnum::GET_FORM, &get("a4", 1)).await, 0);
    assert_eq!(a.rest, [0]);
    assert_eq!(string_at(a.buf.as_ref().unwrap(), 0, 4), "A4");
    let a = info_answer(&f.ok(opnum::GET_FORM, &get("Foolscap", 1)).await, 0);
    assert_eq!(a.rest, [codes::ERROR_INVALID_FORM_NAME]);
}

fn start_doc_request(handle: &[u8; 20], name: &str, datatype: Option<&str>) -> Vec<u8> {
    with_handle(handle, |w| {
        w.u32(1);
        w.u32(1);
        w.pointer(true);
        w.pointer(true);
        w.pointer(false);
        w.pointer(datatype.is_some());
        w.string(name);
        if let Some(datatype) = datatype {
            w.string(datatype);
        }
    })
}

fn write_request(handle: &[u8; 20], data: &[u8]) -> Vec<u8> {
    with_handle(handle, |w| {
        w.bytes(data);
        w.u32(data.len() as u32);
    })
}

/// The DWORDs of a response stub.
fn dwords(out: &[u8]) -> Vec<u32> {
    out.chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

#[tokio::test]
async fn a_document_goes_to_the_print_service() {
    let mut f = Fixture::new();
    let handle = f.printer("PRT01").await;
    let out = f
        .ok(opnum::WRITE_PRINTER, &write_request(&handle, b"x"))
        .await;
    assert_eq!(dwords(&out), [0, codes::ERROR_SPL_NO_STARTDOC]);
    let out = f
        .ok(
            opnum::START_DOC_PRINTER,
            &start_doc_request(&handle, "Test Page", Some("RAW")),
        )
        .await;
    assert_eq!(dwords(&out), [1, 0]);
    let again = f
        .ok(
            opnum::START_DOC_PRINTER,
            &start_doc_request(&handle, "Two", None),
        )
        .await;
    assert_eq!(dwords(&again), [0, codes::ERROR_INVALID_PRINTER_STATE]);
    for _ in 0..2 {
        assert_eq!(
            dwords(
                &f.ok(opnum::START_PAGE_PRINTER, &with_handle(&handle, |_| {}))
                    .await
            ),
            [0]
        );
        let out = f
            .ok(
                opnum::WRITE_PRINTER,
                &write_request(&handle, b"\x1bE page\x0c"),
            )
            .await;
        assert_eq!(dwords(&out), [8, 0]);
        assert_eq!(
            dwords(
                &f.ok(opnum::END_PAGE_PRINTER, &with_handle(&handle, |_| {}))
                    .await
            ),
            [0]
        );
    }
    assert_eq!(
        dwords(
            &f.ok(opnum::END_DOC_PRINTER, &with_handle(&handle, |_| {}))
                .await
        ),
        [0]
    );
    assert_eq!(
        dwords(
            &f.ok(opnum::END_DOC_PRINTER, &with_handle(&handle, |_| {}))
                .await
        ),
        [codes::ERROR_SPL_NO_STARTDOC]
    );
    let printed = f.print.printed();
    assert_eq!(printed.len(), 1);
    let job = &printed[0];
    assert_eq!((job.share.as_str(), job.user.as_str()), ("PRT01", "ALICE"));
    assert_eq!(job.doc.document, "Test Page");
    assert_eq!(job.doc.machine, "DESKTOP-1");
    assert_eq!(job.data, b"\x1bE page\x0c\x1bE page\x0c");
    assert_eq!(job.ended, Some(2));
}

#[tokio::test]
async fn documents_refuse_other_data_types_and_output_files() {
    let mut f = Fixture::new();
    let handle = f.printer("PRT01").await;
    let out = f
        .ok(
            opnum::START_DOC_PRINTER,
            &start_doc_request(&handle, "d", Some("NT EMF 1.008")),
        )
        .await;
    assert_eq!(dwords(&out), [0, codes::ERROR_INVALID_DATATYPE]);
    let file = with_handle(&handle, |w| {
        w.u32(1);
        w.u32(1);
        w.pointer(true);
        w.pointer(true);
        w.pointer(true);
        w.pointer(false);
        w.string("d");
        w.string("C:\\out.prn");
    });
    assert_eq!(
        dwords(&f.ok(opnum::START_DOC_PRINTER, &file).await),
        [0, codes::ERROR_INVALID_PARAMETER]
    );
    let level2 = with_handle(&handle, |w| {
        w.u32(2);
        w.u32(2);
        w.pointer(false);
    });
    assert_eq!(
        dwords(&f.ok(opnum::START_DOC_PRINTER, &level2).await),
        [0, codes::ERROR_INVALID_LEVEL]
    );
    let server = f.open("\\\\QSHQ400", 0x2).await.0;
    let out = f
        .ok(
            opnum::START_DOC_PRINTER,
            &start_doc_request(&server, "d", None),
        )
        .await;
    assert_eq!(dwords(&out), [0, codes::ERROR_INVALID_HANDLE]);
    assert!(f.print.printed().is_empty());
}

#[tokio::test]
async fn abort_abandons_and_close_ends_an_open_document() {
    let mut f = Fixture::new();
    let a = f.printer("PRT01").await;
    let b = f.printer("PRT01").await;
    for handle in [&a, &b] {
        f.ok(
            opnum::START_DOC_PRINTER,
            &start_doc_request(handle, "d", None),
        )
        .await;
        f.ok(opnum::WRITE_PRINTER, &write_request(handle, b"data"))
            .await;
    }
    assert_eq!(
        dwords(&f.ok(opnum::ABORT_PRINTER, &with_handle(&a, |_| {})).await),
        [0]
    );
    f.ok(opnum::CLOSE_PRINTER, &with_handle(&b, |_| {})).await;
    let printed = f.print.printed();
    assert!(printed[0].abandoned && printed[0].ended.is_none());
    assert_eq!(printed[1].ended, Some(0));
    // A document still open when the pipe goes is abandoned.
    let c = f.printer("LASER").await;
    f.ok(opnum::START_DOC_PRINTER, &start_doc_request(&c, "d", None))
        .await;
    drop(f.session);
    assert!(f.print.printed()[2].abandoned);
}

#[tokio::test]
async fn jobs_are_the_users_own() {
    let mut f = Fixture::new();
    let handle = f.printer("PRT01").await;
    f.ok(
        opnum::START_DOC_PRINTER,
        &start_doc_request(&handle, "Mine", None),
    )
    .await;
    f.ok(opnum::END_DOC_PRINTER, &with_handle(&handle, |_| {}))
        .await;
    for level in [1, 2] {
        let (buf, rest) = sized(&mut f, opnum::ENUM_JOBS, 1, |len| {
            with_handle(&handle, |w| {
                w.u32(0);
                w.u32(10);
                w.u32(level);
                buffer(w, len);
            })
        })
        .await;
        assert_eq!(rest, [1, 0]);
        assert_eq!(u32_at(&buf, 0), 1);
        assert_eq!(string_at(&buf, 0, 12), "ALICE");
        assert_eq!(string_at(&buf, 0, 16), "Mine");
        let (buf, rest) = sized(&mut f, opnum::GET_JOB, 0, |len| {
            with_handle(&handle, |w| {
                w.u32(1);
                w.u32(level);
                buffer(w, len);
            })
        })
        .await;
        assert_eq!(rest, [0]);
        assert_eq!(string_at(&buf, 0, 4), "\\\\QSHQ400\\PRT01");
    }
    let missing = with_handle(&handle, |w| {
        w.u32(99);
        w.u32(1);
        buffer(w, Some(512));
    });
    let a = info_answer(&f.ok(opnum::GET_JOB, &missing).await, 0);
    assert_eq!(a.rest, [codes::ERROR_INVALID_PARAMETER]);
    f.who = Identity::User {
        user: "BOB".into(),
        domain: String::new(),
    };
    let bobs = with_handle(&handle, |w| {
        w.u32(0);
        w.u32(10);
        w.u32(1);
        buffer(w, Some(512));
    });
    let a = info_answer(&f.ok(opnum::ENUM_JOBS, &bobs).await, 1);
    assert_eq!(a.rest, [0, 0]);
}

fn data_request(handle: &[u8; 20], key: Option<&str>, name: &str, size: u32) -> Vec<u8> {
    with_handle(handle, |w| {
        if let Some(key) = key {
            w.string(key);
        }
        w.string(name);
        w.u32(size);
    })
}

/// pType, pData, pcbNeeded and the status of GetPrinterData(Ex).
fn data_answer(out: &[u8]) -> (u32, Vec<u8>, u32, u32) {
    let mut r = NdrReader::new(out);
    let kind = r.u32().unwrap();
    let data = r.bytes(1 << 16).unwrap().to_vec();
    let needed = r.u32().unwrap();
    let status = r.u32().unwrap();
    assert!(r.at_end());
    (kind, data, needed, status)
}

#[tokio::test]
async fn printer_data_answers_what_clients_ask() {
    let mut f = Fixture::new();
    let server = f.open("\\\\QSHQ400", 0x2).await.0;
    let out = f
        .ok(
            opnum::GET_PRINTER_DATA,
            &data_request(&server, None, "MajorVersion", 4),
        )
        .await;
    assert_eq!(data_answer(&out), (4, vec![3, 0, 0, 0], 4, 0));
    let out = f
        .ok(
            opnum::GET_PRINTER_DATA,
            &data_request(&server, None, "Architecture", 0),
        )
        .await;
    assert_eq!(data_answer(&out), (1, vec![], 24, codes::ERROR_MORE_DATA));
    let out = f
        .ok(
            opnum::GET_PRINTER_DATA,
            &data_request(&server, None, "OSVersion", 276),
        )
        .await;
    assert_eq!(data_answer(&out).1[4], 10);
    let out = f
        .ok(
            opnum::GET_PRINTER_DATA,
            &data_request(&server, None, "Nope", 16),
        )
        .await;
    assert_eq!(
        data_answer(&out),
        (0, vec![0; 16], 0, codes::ERROR_FILE_NOT_FOUND)
    );

    let printer = f.printer("PRT01").await;
    let out = f
        .ok(
            opnum::GET_PRINTER_DATA,
            &data_request(&printer, None, "ChangeID", 4),
        )
        .await;
    let (kind, data, ..) = data_answer(&out);
    assert_eq!(kind, 4);
    let again = f
        .ok(
            opnum::GET_PRINTER_DATA,
            &data_request(&printer, None, "ChangeID", 4),
        )
        .await;
    assert_eq!(data_answer(&again).1, data, "stable");
    let out = f
        .ok(
            opnum::GET_PRINTER_DATA_EX,
            &data_request(&printer, Some("PrinterDriverData"), "Model", 64),
        )
        .await;
    assert_eq!(data_answer(&out).3, codes::ERROR_FILE_NOT_FOUND);
    let out = f
        .ok(
            opnum::GET_PRINTER_DATA_EX,
            &data_request(&printer, Some("DsSpooler"), "ChangeID", 4),
        )
        .await;
    assert_eq!(data_answer(&out).3, codes::ERROR_FILE_NOT_FOUND);

    let enum_ex = |key: &str| {
        with_handle(&printer, |w| {
            w.string(key);
            w.u32(32);
        })
    };
    let out = f
        .ok(opnum::ENUM_PRINTER_DATA_EX, &enum_ex("PrinterDriverData"))
        .await;
    let mut r = NdrReader::new(&out);
    assert_eq!(r.bytes(64).unwrap(), &[0; 32]);
    assert_eq!([r.u32(), r.u32(), r.u32()], [Some(0), Some(0), Some(0)]);
    let out = f
        .ok(opnum::ENUM_PRINTER_DATA_EX, &enum_ex("DsDriver"))
        .await;
    assert_eq!(u32_at(&out, out.len() - 4), codes::ERROR_FILE_NOT_FOUND);

    let key = with_handle(&printer, |w| {
        w.string("");
        w.u32(64);
    });
    let out = f.ok(opnum::ENUM_PRINTER_KEY, &key).await;
    assert_eq!(u32_at(&out, 0), 32, "units");
    assert_eq!(
        &out[4..42],
        [super::marshal::utf16z("PrinterDriverData"), vec![0, 0]].concat()
    );
    assert_eq!(dwords(&out[68..]), [38, 0]);

    let index = |i: u32| {
        with_handle(&server, |w| {
            w.u32(i);
            w.u32(64);
            w.u32(512);
        })
    };
    let out = f.ok(opnum::ENUM_PRINTER_DATA, &index(0)).await;
    assert_eq!(u32_at(&out, out.len() - 4), 0);
    let out = f.ok(opnum::ENUM_PRINTER_DATA, &index(99)).await;
    assert_eq!(u32_at(&out, out.len() - 4), codes::ERROR_NO_MORE_ITEMS);
}

#[tokio::test]
async fn notifications_are_refused_so_clients_poll() {
    let mut f = Fixture::new();
    let handle = f.printer("PRT01").await;
    let first_ex = with_handle(&handle, |w| {
        w.u32(0x0000_FFFF);
        w.u32(0);
        w.pointer(true);
        w.string("\\\\DESKTOP-1");
        w.u32(7);
        w.pointer(false);
    });
    let out = f
        .ok(
            opnum::REMOTE_FIND_FIRST_PRINTER_CHANGE_NOTIFICATION_EX,
            &first_ex,
        )
        .await;
    assert_eq!(dwords(&out), [codes::ERROR_NOT_SUPPORTED]);
    let first = with_handle(&handle, |w| {
        w.u32(0x0000_FFFF);
        w.u32(0);
        w.pointer(false);
        w.u32(7);
        w.u32(4);
        w.pointer(true);
        w.bytes(&[1, 2, 3, 4]);
    });
    let out = f
        .ok(opnum::REMOTE_FIND_FIRST_PRINTER_CHANGE_NOTIFICATION, &first)
        .await;
    assert_eq!(&out[4..12], &[4, 0, 0, 0, 1, 2, 3, 4]);
    assert_eq!(u32_at(&out, 12), codes::ERROR_NOT_SUPPORTED);
    let close = f
        .ok(
            opnum::FIND_CLOSE_PRINTER_CHANGE_NOTIFICATION,
            &with_handle(&handle, |_| {}),
        )
        .await;
    assert_eq!(dwords(&close), [codes::ERROR_INVALID_HANDLE]);
}

#[tokio::test]
async fn administration_is_denied_and_other_operations_are_out_of_range() {
    let mut f = Fixture::new();
    let handle = f.printer("PRT01").await;
    for op in ADMINISTRATION {
        let fault = f.call(op, &with_handle(&handle, |_| {})).await.unwrap_err();
        assert_eq!(fault.status, codes::ERROR_ACCESS_DENIED, "opnum {op}");
    }
    for op in [
        10, 12, 15, 22, 24, 28, 35, 36, 51, 58, 96, 102, 104, 130, 400,
    ] {
        let fault = f.call(op, &with_handle(&handle, |_| {})).await.unwrap_err();
        assert_eq!(fault.status, NCA_OP_RNG_ERROR, "opnum {op}");
    }
    for op in [opnum::SET_JOB, opnum::SCHEDULE_JOB, opnum::RESET_PRINTER] {
        let out = f.ok(op, &with_handle(&handle, |w| w.u32(0))).await;
        assert_eq!(dwords(&out), [codes::ERROR_NOT_SUPPORTED]);
    }
}

#[tokio::test]
async fn bad_stubs_and_unknown_handles_fault() {
    let mut f = Fixture::new();
    let handle = f.printer("PRT01").await;
    let fault = f.call(opnum::GET_PRINTER, &[0; 8]).await.unwrap_err();
    assert_eq!(fault.status, BAD_STUB_DATA);
    let unknown = with_handle(&[7; 20], |w| {
        w.u32(2);
        buffer(w, None);
    });
    assert_eq!(
        f.call(opnum::GET_PRINTER, &unknown)
            .await
            .unwrap_err()
            .status,
        CONTEXT_MISMATCH
    );
    let huge = with_handle(&handle, |w| {
        w.u32(2);
        w.pointer(false);
        w.u32(MAX_OUT_BUFFER as u32 + 1);
    });
    assert_eq!(
        f.call(opnum::GET_PRINTER, &huge).await.unwrap_err().status,
        BAD_STUB_DATA
    );
    let trailing = [
        with_handle(&handle, |w| {
            w.u32(2);
            buffer(w, None);
        }),
        vec![0; 4],
    ]
    .concat();
    assert_eq!(
        f.call(opnum::GET_PRINTER, &trailing)
            .await
            .unwrap_err()
            .status,
        BAD_STUB_DATA
    );
    let short_write = with_handle(&handle, |w| {
        w.bytes(b"abc");
        w.u32(4);
    });
    assert_eq!(
        f.call(opnum::WRITE_PRINTER, &short_write)
            .await
            .unwrap_err()
            .status,
        BAD_STUB_DATA
    );
}
