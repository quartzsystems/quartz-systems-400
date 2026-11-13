// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `nfs4` fuzz entry point without instrumentation: compounds built
//! from seeds that carry the export's root handle (0.0, 4.1 with and
//! without a session, opens, locks, reads, writes, directory changes),
//! mutated; nothing panics and every reply's results decode.

use std::time::Instant;

use nfs::fuzzing;
use nfs::types::nfs4::{self as t, nfs_argop4};
use nfs::v4::attr::Bits;
use nfs::xdr;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n.max(1) as u64)).unwrap_or(0)
    }
}

fn root() -> nfs_argop4 {
    nfs_argop4::OP_PUTFH(t::PUTFH4args {
        object: t::nfs_fh4(vec![0xAA; 32]),
    })
}

fn name(s: &str) -> t::utf8string {
    t::utf8string(s.as_bytes().to_vec())
}

fn anon() -> t::stateid4 {
    t::stateid4 {
        seqid: 0,
        other: [0; 12],
    }
}

fn compound(minor: u32, ops: Vec<nfs_argop4>) -> Vec<u8> {
    // Procedure 1, then a selector byte (AUTH_SYS as UID 100).
    let mut v = vec![1, 0x20];
    v.extend(
        xdr::to_bytes(&t::COMPOUND4args {
            tag: name("f"),
            minorversion: minor,
            argarray: ops,
        })
        .unwrap(),
    );
    v
}

fn seeds() -> Vec<Vec<u8>> {
    let owner = t::state_owner4 {
        clientid: 1,
        owner: b"o".to_vec(),
    };
    let open = nfs_argop4::OP_OPEN(t::OPEN4args {
        seqid: 1,
        share_access: 3,
        share_deny: 0,
        owner: owner.clone(),
        openhow: t::openflag4::OPEN4_CREATE(t::createhow4::UNCHECKED4(t::fattr4 {
            attrmask: Bits::of(&[t::FATTR4_MODE]).to_bitmap(),
            attr_vals: t::attrlist4(0o644u32.to_be_bytes().to_vec()),
        })),
        claim: t::open_claim4::CLAIM_NULL(name("n")),
    });
    let getattr = nfs_argop4::OP_GETATTR(t::GETATTR4args {
        attr_request: Bits::of(&[1, 3, 4, 8, 20, 33, 36, 37, 47, 53, 55, 75]).to_bitmap(),
    });
    let lock = nfs_argop4::OP_LOCK(t::LOCK4args {
        locktype: t::nfs_lock_type4::WRITE_LT,
        reclaim: false,
        offset: 0,
        length: 10,
        locker: t::locker4::TRUE(t::open_to_lock_owner4 {
            open_seqid: 2,
            open_stateid: t::stateid4 {
                seqid: 1,
                other: [0; 12],
            },
            lock_seqid: 0,
            lock_owner: owner,
        }),
    });
    vec![
        compound(0, vec![root(), getattr.clone(), nfs_argop4::OP_GETFH]),
        compound(0, vec![nfs_argop4::OP_PUTROOTFH, nfs_argop4::OP_LOOKUPP]),
        compound(
            0,
            vec![
                nfs_argop4::OP_SETCLIENTID(t::SETCLIENTID4args {
                    client: t::nfs_client_id4 {
                        verifier: [1; 8],
                        id: b"fz".to_vec(),
                    },
                    callback: t::cb_client4 {
                        cb_program: 1,
                        cb_location: t::netaddr4 {
                            na_r_netid: b"tcp".to_vec(),
                            na_r_addr: b"127.0.0.1.0.1".to_vec(),
                        },
                    },
                    callback_ident: 1,
                }),
            ],
        ),
        compound(0, vec![root(), open, nfs_argop4::OP_GETFH, lock]),
        compound(
            0,
            vec![
                root(),
                nfs_argop4::OP_LOOKUP(t::LOOKUP4args { objname: name("f") }),
                nfs_argop4::OP_READ(t::READ4args {
                    stateid: anon(),
                    offset: 0,
                    count: 100,
                }),
                nfs_argop4::OP_WRITE(t::WRITE4args {
                    stateid: anon(),
                    offset: 3,
                    stable: t::stable_how4::UNSTABLE4,
                    data: b"xyz".to_vec(),
                }),
                nfs_argop4::OP_COMMIT(t::COMMIT4args {
                    offset: 0,
                    count: 0,
                }),
            ],
        ),
        compound(
            0,
            vec![
                root(),
                nfs_argop4::OP_READDIR(t::READDIR4args {
                    cookie: 0,
                    cookieverf: [0; 8],
                    dircount: 100,
                    maxcount: 1000,
                    attr_request: Bits::of(&[1, 11, 19, 20]).to_bitmap(),
                }),
                nfs_argop4::OP_CREATE(t::CREATE4args {
                    objtype: t::createtype4::NF4DIR,
                    objname: name("nd"),
                    createattrs: t::fattr4 {
                        attrmask: t::bitmap4(vec![]),
                        attr_vals: t::attrlist4(vec![]),
                    },
                }),
                nfs_argop4::OP_SAVEFH,
                root(),
                nfs_argop4::OP_RENAME(t::RENAME4args {
                    oldname: name("d"),
                    newname: name("e"),
                }),
            ],
        ),
        compound(
            1,
            vec![nfs_argop4::OP_EXCHANGE_ID(t::EXCHANGE_ID4args {
                eia_clientowner: t::client_owner4 {
                    co_verifier: [1; 8],
                    co_ownerid: b"fz".to_vec(),
                },
                eia_flags: 0,
                eia_state_protect: t::state_protect4_a::SP4_NONE,
                eia_client_impl_id: vec![],
            })],
        ),
        compound(
            1,
            vec![
                nfs_argop4::OP_SEQUENCE(t::SEQUENCE4args {
                    sa_sessionid: [0; 16],
                    sa_sequenceid: 1,
                    sa_slotid: 0,
                    sa_highest_slotid: 0,
                    sa_cachethis: true,
                }),
                root(),
                getattr,
            ],
        ),
        compound(2, vec![root()]),
    ]
}

#[test]
fn nfs4_compounds_mutated() {
    let seeds = seeds();
    let mut rng = Rng(0x5eed_4444);
    let end = Instant::now() + std::time::Duration::from_secs(20);
    for s in &seeds {
        fuzzing::nfs4(s);
    }
    let mut n = 0;
    while n < 20_000 && Instant::now() < end {
        let mut b = seeds[rng.below(seeds.len())].clone();
        for _ in 0..=rng.below(4) {
            let i = rng.below(b.len());
            match rng.below(4) {
                0 => b[i] ^= 1 << rng.below(8),
                1 => b[i] = u8::try_from(rng.below(256)).unwrap(),
                2 => b.truncate(i.max(2)),
                _ => b.insert(i, 0),
            }
        }
        fuzzing::nfs4(&b);
        n += 1;
    }
    assert!(n > 1000);
}
