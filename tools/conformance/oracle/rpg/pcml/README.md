# PCML captures

What `CRTBNDRPG PGMINFO(*PCML) INFOSTMF(...)` and `CRTRPGMOD` wrote on
pub400.com (IBM i 7.5) on 2026-11-04 for the sources in
`rpg/compiler/src/pcml.rs`'s tests, compiled into the account's library
(deleted after, with the source file and the stream files). The account is
written `{user}`. `cargo test -p rpgc --lib pcml` compares every line but
the time with what QS/400 writes.
