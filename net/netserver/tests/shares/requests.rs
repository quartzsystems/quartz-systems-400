// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What each SMB operation costs in requests to the storage service: the
//! job's IFS client counted (`ifs::client::Counted`, one call one
//! request on a job's link), over one IFS in memory. Each figure is the
//! most the backend may send; `docs/NETSERVER.md` section 11 has them.

use std::sync::atomic::AtomicU64;

use ifs::client::Counted;

use super::*;

/// A job whose IFS calls are counted, with leases in the table.
fn counted_job(fs: &Arc<Fs>, user: &str) -> (Job, Arc<AtomicU64>) {
    let calls = Arc::new(AtomicU64::new(0));
    let owner = fs.owner.fetch_add(1, Ordering::SeqCst);
    let (f, u, n) = (fs.clone(), user.to_owned(), calls.clone());
    let server = NetServer::build(
        &config(),
        Box::new(move || {
            Box::new(Counted {
                inner: f.client_as(&u, owner),
                calls: n,
            }) as Box<dyn IfsClient>
        }),
        Arc::new(Profiles),
        &(Arc::new(Spool(Printed::default())) as Arc<dyn Printer>),
    )
    .unwrap();
    let (f, u) = (fs.clone(), user.to_owned());
    server
        .start_leases(Box::new(move || {
            Box::new(f.client_as(&u, owner)) as Box<dyn IfsClient>
        }))
        .unwrap();
    let server = Arc::new(server);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let s = server.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { break };
            let s = s.clone();
            std::thread::spawn(move || {
                let _ = s.serve_one(stream);
            });
        }
    });
    let job = Job {
        server,
        addr,
        printed: Printed::default(),
    };
    (job, calls)
}

/// The calls made since the last look, once the ones queued behind the
/// operation (a lease report) are made.
fn since(calls: &AtomicU64, last: &mut u64) -> u64 {
    let mut now = calls.load(Ordering::SeqCst);
    loop {
        std::thread::sleep(Duration::from_millis(30));
        let again = calls.load(Ordering::SeqCst);
        if again == now {
            break;
        }
        now = again;
    }
    let n = now - *last;
    *last = now;
    n
}

#[tokio::test(flavor = "multi_thread")]
async fn requests_per_operation() {
    let fs = Fs::new();
    let me = fs.client("ALICE");
    for i in 0..50 {
        let fd = me
            .open(
                &format!("/home/ALICE/f{i:02}.txt"),
                OpenArgs::new(O_CREAT | O_RDWR).mode(0o644),
            )
            .unwrap();
        me.write(fd, &[b'x'; 100]).unwrap();
        me.close(fd).unwrap();
    }
    let (j, calls) = counted_job(&fs, "ALICE");
    let mut c = alice(j.addr, "HOME").await;
    let mut last = calls.load(Ordering::SeqCst);
    let mut seen = Vec::new();
    let mut note = |what: &str, n: u64| seen.push(format!("{what}: {n}"));

    // A new file: CREATE with a lease, a 4 KiB WRITE, CLOSE.
    let id = c
        .create(Create {
            disposition: FILE_CREATE,
            oplock: OPLOCK_LEVEL_LEASE,
            contexts: contexts(&[lease_v1([1; 16], RWH)]),
            ..mk("new.txt")
        })
        .await
        .unwrap()
        .file_id;
    note("create", since(&calls, &mut last));
    assert_eq!(c.write(id, 0, &[7; 4096]).await.unwrap(), 4096);
    note("write", since(&calls, &mut last));
    c.close(id).await;
    note("close", since(&calls, &mut last));

    // An existing file: open to read, its size, READ, CLOSE.
    let id = c
        .open("f07.txt", FILE_GENERIC_READ, SHARE_ALL)
        .await
        .unwrap();
    note("open", since(&calls, &mut last));
    c.query_file_info(id, FILE_STANDARD_INFORMATION)
        .await
        .unwrap();
    note("query info", since(&calls, &mut last));
    c.read(id, 0, 100).await.unwrap();
    note("read", since(&calls, &mut last));
    c.close(id).await;
    note("close after read", since(&calls, &mut last));

    // A listing of 51 files.
    let dir = c.open_dir("").await;
    note("open dir", since(&calls, &mut last));
    assert_eq!(c.list(dir, 0, "*").await.unwrap().len(), 53);
    note("list 51", since(&calls, &mut last));
    c.close(dir).await;
    let _ = since(&calls, &mut last);
    println!("{}", seen.join(", "));
    // Before: 9, 1, 2, 6, 5, 1, 1, 5 and 108 (two per entry).
    assert_eq!(
        seen,
        [
            "create: 5",
            "write: 1",
            "close: 2",
            "open: 2",
            "query info: 2",
            "read: 1",
            "close after read: 1",
            "open dir: 1",
            "list 51: 2",
        ]
    );
    me.release();
}
