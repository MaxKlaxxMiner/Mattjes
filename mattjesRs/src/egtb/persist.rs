//! The cache, see `mattjesGo/egtb/persist.go`: a directory next to the binary
//! with the base file (all tables up to four pieces in the order of
//! `Material::all()`) and one file per larger material ("<pieces>-<name>.bin",
//! never shipped, only generated locally).
//!
//! File format (v3, 2026-10-16): 4 bytes magic "MEG3", 1 byte step (the
//! resolution of the values in moves, `Table::step`: 1 = exact, n = buckets of
//! n moves), 1 byte value width (0 or 1 = one byte per entry, 2 = a wide
//! table, since 2026-10-26: the container then holds 2n bytes, the low plane
//! of all entries, then the high plane, `Value::to_planes`), 2 bytes zero, 8
//! bytes raw checksum (the constant in the code, over
//! the exact values as generated, dead and illegal entries as 0), then the
//! indexed lz container with the values (`lz::pack_indexed_to`: block offsets
//! up front, so a table beyond the base is read block by block through a
//! block cache instead of being unpacked as a whole; the base uses 4 MB blocks
//! and is unpacked, the rest `BLOCK_BYTES`). Before compression every dead or
//! illegal entry is replaced by its predecessor: those positions are never
//! looked up, so their content is free and the runs get longer. Tables beyond
//! the base are written quantised to `FILE_STEP` (the search resolves the
//! exact distance from the bounds, docs/m5-endgame-tables.md); the base is
//! always exact. Go and Rust must produce byte-identical files.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::generate::{Progress, Stats};
use super::table::{Set, Table, Value};
use super::Material;
use crate::lz;

pub const CACHE_DIR: &str = "mattjes-egtb-cache";

/// The cache folder holds the base file at its root; the tables beyond the
/// base live in two subfolders: the (quantised) distance tables and our own
/// WDL tables in Syzygy's structure.
pub const DTM_DIR: &str = "dtm";
pub const WDL_DIR: &str = "wdl";
pub const BASE_FILE_NAME: &str = "4-all.bin";
const MAGIC: &[u8; 4] = b"MEG3";
const HEADER_BYTES: usize = 16;
/// The raw block size of the files beyond the base. Measured on KRRKN
/// (docs/m5-endgame-tables.md, "Blockformat"): 64 KB costs 5 % file size over
/// 4 MB blocks but a search touches 45 % of the table, 16 KB costs 17 % and
/// touches 27 %, 4 KB costs 40 % and touches 15 %; a miss decodes one block,
/// 16 KB in about 18 µs. 16 KB is the compromise.
pub const BLOCK_BYTES: usize = 16 << 10;

/// The resolution in moves of every table beyond the base in the cache files
/// ("mate in at most 8 moves" instead of the exact count). It halves the files
/// twice over against exact values; the search resolves the 16-ply window in
/// about 0.3 s at the root (KRR-KN, KQ-KBN), step 4 would take 1 ms and save
/// one halving less. The base stays exact (37 MB), and the generator needs
/// exact dependencies, so a quantised cache file is never used as one
/// (`generate_controlled` regenerates it in memory instead).
pub const FILE_STEP: u32 = 8;

/// Marks the exact twin of a cache file ("5-KBNKQ.bin.raw"): the same
/// container, but step 1. A generation writes it for every dependency it had
/// to compute, so that an interrupted job keeps them and a later generation
/// can load them instead of computing them again (the quantised file cannot
/// serve as a dependency). The user deletes them by hand.
pub const RAW_SUFFIX: &str = ".raw";

/// Threads used to compress and decompress cache files (independent 4 MB blocks).
pub const IO_WORKERS: usize = 8;

/// The checksum of the complete base (a hash of the 35 table checksums). It
/// doubles as the regression test of the generator: a changed generator that
/// still produces this value produces the same tables. 0 means "not yet
/// recorded".
pub const FILE_CHECKSUM: u64 = 0x64fe872c7e217f6e; // format v3 (2026-10-08): losses up to 254 plies, no invalid value

/// The raw checksums of the single tables (format v3: move count per byte,
/// dead and illegal entries 0) and compared after every generation and load.
/// Larger materials are added as they are measured; the five-piece values of
/// format v2 (2026-10-07) are in the git history and in
/// docs/m5-endgame-tables.md.
pub const TABLE_CHECKSUMS: &[(&str, u64)] = &[
    ("KQK", 0x9260104b203b89c4),
    ("KRK", 0x5387c89faf9e0217),
    ("KBK", 0x73f143eadfe811a8),
    ("KNK", 0x73f143eadfe811a8), // = KBK: all zero
    ("KQQK", 0xea8550ab91d6bf04),
    ("KQRK", 0x7e56203083caed07),
    ("KQBK", 0x0daa2fa637ffbf47),
    ("KQNK", 0xfa22d9cbcc5750d2),
    ("KRRK", 0x9fa46d416e96e031),
    ("KRBK", 0x0fcf7cb7a35f16b7),
    ("KRNK", 0xf61bbd3f8e767a38),
    ("KBBK", 0x14956d0d156a61b2),
    ("KBNK", 0x3c55de9ecf98feb6),
    ("KNNK", 0xf3fe22b4f30f1651),
    ("KQKQ", 0xda1249305b769fb7),
    ("KQKR", 0xb7d88c4c91896ef9),
    ("KQKB", 0x2a2334b0f5d15047),
    ("KQKN", 0x2e37a9157d56f895),
    ("KRKR", 0xfb8ee6c6d503053c),
    ("KRKB", 0xefa63d9931328cda),
    ("KRKN", 0xc020cf7e4db95b94),
    ("KBKB", 0x5fef424e80401f69),
    ("KBKN", 0x5f096e24292f2568),
    ("KNKN", 0x667f4a21cc8aaab2),
    ("KPK", 0xb1d19b5318c5076c),
    ("KQPK", 0x1c74eb840969f447),
    ("KRPK", 0x05107c598fafd5ec),
    ("KBPK", 0x7943932114500468),
    ("KNPK", 0xa6464dc2cc6a01f8),
    ("KQKP", 0x24b8b54772f35819),
    ("KRKP", 0x57b1f53eaee0a89c),
    ("KBKP", 0x74931eaafaa29dd8),
    ("KNKP", 0x627ee5ef42b600f5),
    ("KPPK", 0x829f48ba8cf92ce4),
    ("KPKP", 0xae084110af4ab131),
    // five pieces, all 110 (format v3; from the sweep of 2026-10-23 on the home machine, Rust,
    // measure.log; KBBBK, KBNKQ and KRRKN agree with the earlier Go and Rust runs)
    ("KBBBK", 0xa552a70f5be1c3db), ("KBBKB", 0x72234cc21a7defaf), ("KBBKN", 0xcac529d6b477774b),
    ("KBBKQ", 0xd12a7747cc7d9f45), ("KBBKR", 0x25fcfbb6a10810e8), ("KBBNK", 0x234d18bcb557ad7f),
    ("KBNKB", 0xfd2f437a0a54b862), ("KBNKN", 0x12113e9a5e7c4e48), ("KBNKQ", 0xa03df227d9f5eb72),
    ("KBNKR", 0x187c046383539781), ("KBNNK", 0xda5cd512f702000a), ("KNNKB", 0xdaa5176ca837e544),
    ("KNNKN", 0x2cadd8a85308b7cf), ("KNNKQ", 0xb723b4fae7715b91), ("KNNKR", 0x3b77145d4f6957c5),
    ("KNNNK", 0xa44c31959604c935), ("KQBBK", 0x54972d4b84afa912), ("KQBKB", 0x08c8342dc0e58d3e),
    ("KQBKN", 0xd1fbb25d97b6f3a3), ("KQBKQ", 0x3aa37ee109fb585f), ("KQBKR", 0x0be7752be640c86c),
    ("KQBNK", 0x81586b6e6f1ccc6d), ("KQNKB", 0xa342715277bab8c6), ("KQNKN", 0x10749f36888b538a),
    ("KQNKQ", 0x878542d0f0bdfe00), ("KQNKR", 0x9f4a7fdb8db76e86), ("KQNNK", 0xfafc1f102f4d3edb),
    ("KQQBK", 0x6b79d5e753baa689), ("KQQKB", 0xfed65a59f7c286cf), ("KQQKN", 0x5eaeaf8457ca0e25),
    ("KQQKQ", 0x32f36f5b986ac0de), ("KQQKR", 0x67d79a19f27e30ad), ("KQQNK", 0xd7696e03b378e695),
    ("KQQQK", 0x26ccd333c5249437), ("KQQRK", 0x46018430785e3443), ("KQRBK", 0x24bdc7ea83a73e37),
    ("KQRKB", 0xaa14ca6028d40b9a), ("KQRKN", 0xc67bd8787b1cb2d4), ("KQRKQ", 0x9ace8ab5334edd2f),
    ("KQRKR", 0x280529af63d896c6), ("KQRNK", 0x2e0b74455ddcdb70), ("KQRRK", 0x5925eaafa071bf0d),
    ("KRBBK", 0x8973a57db42adaae), ("KRBKB", 0xec0971f832a3ec14), ("KRBKN", 0x8a0c296adea78498),
    ("KRBKQ", 0x37b8a37e2578a4c4), ("KRBKR", 0xf1eeeb2878de49a7), ("KRBNK", 0x777caf348dd076b9),
    ("KRNKB", 0x74b159efc1a16d9c), ("KRNKN", 0xf8abb5381fb82d8a), ("KRNKQ", 0x5ebac3e35ae6129d),
    ("KRNKR", 0xf176012ec8aa7c0f), ("KRNNK", 0x8067f92cf8f05172), ("KRRBK", 0x955121c5e7f34b2a),
    ("KRRKB", 0x1b2da1009c1b09ab), ("KRRKN", 0x443a1b3b48954ec3), ("KRRKQ", 0xd7a15047fa0ad275),
    ("KRRKR", 0xbe96f115eba44051), ("KRRNK", 0xa75e70e701bbf8ac), ("KRRRK", 0x38031bfa0e98bbee),
    ("KBBKP", 0x11ded18695795d6c), ("KBBPK", 0x1e90b35dd894b11e), ("KBNKP", 0xfe0d3bb9d3e5667f),
    ("KBNPK", 0xcc3f466efcc94af1), ("KBPKB", 0x3d83b0fd492bc18e), ("KBPKN", 0x4000947a38cd27ea),
    ("KBPKQ", 0x5365d229c58e74e2), ("KBPKR", 0xc9718c864cbcc955), ("KNNKP", 0x2fe018737a9a2387),
    ("KNNPK", 0xd83c464672e30fe1), ("KNPKB", 0x71772e64bd51012a), ("KNPKN", 0x5e0bfb8e51dd84e5),
    ("KNPKQ", 0x42deff8d914c91da), ("KNPKR", 0xc48002f00bdb4b09), ("KQBKP", 0x5fe49081bd07c0d9),
    ("KQBPK", 0xca397e89179e8f73), ("KQNKP", 0x835f7e19d227df05), ("KQNPK", 0x36e0f2e1db352887),
    ("KQPKB", 0x4828f117c85b5f39), ("KQPKN", 0xb70e06a378558b50), ("KQPKQ", 0x629bb099a6a90e07),
    ("KQPKR", 0xce224ecf69d3fdb4), ("KQQKP", 0xa5674b645afe063b), ("KQQPK", 0x15aa245d511d4a3b),
    ("KQRKP", 0xc3f60f7b8c6ee78a), ("KQRPK", 0x6a3d308b1f468e66), ("KRBKP", 0x23b40adbdf6c4fed),
    ("KRBPK", 0x01df5843fde7058a), ("KRNKP", 0x97ff32bf6d501a1e), ("KRNPK", 0x77e0598d862784ac),
    ("KRPKB", 0x0d3e7a4b90d9e182), ("KRPKN", 0x57d74af89ec77093), ("KRPKQ", 0x3055982894f56ddc),
    ("KRPKR", 0x7cc6ba239baf5da9), ("KRRKP", 0x3003050b376b4546), ("KRRPK", 0x8a1494460fb7f479),
    ("KBPKP", 0x75ea61ed8bf95531), ("KBPPK", 0xecc3de59fb86dff7), ("KNPKP", 0x7c2e668122934016),
    ("KNPPK", 0xba0f257aa2ecc2ed), ("KPPKB", 0xcc9a627ad3f55921), ("KPPKN", 0x5c456acab2bbab19),
    ("KPPKQ", 0x902875aaa6992709), ("KPPKR", 0x4c92580848463cd6), ("KQPKP", 0xfeeca92d9b94747d),
    ("KQPPK", 0x0826d0219e8be912), ("KRPKP", 0x62929c193fa9a46d), ("KRPPK", 0x71e1d1f22a9e53e8),
    // KPPKP as its widened file (two bytes per entry, `WIDE_TABLES`) of the server sweep; the
    // one-byte table of the home machine had 0x87cb3b021b3416b0
    ("KPPKP", 0x89fe302044790610), ("KPPPK", 0x9b0c4f0c85c11d02),
    // six pieces (format v3, Rust, home machine): KRRKBN 1294 s, 35 GiB committed
    ("KRRKBN", 0x11049fde56c4b77c),
    // the other 364 six-piece tables: the raw sweep of 2026-10-26 to 2026-11-05 on the server
    // (measure.log; the en passant fixes 54f14b2 and 34faeee changed none of the tables
    // generated before them, KBPKBP, KNNPKP and KPPKPP were verified with egtb-verify)
    ("KBBBBK", 0xe5ca3311cff94fc7), ("KBBBKB", 0x1764dbd81ef7d983), ("KBBBKN", 0xc52b066e01c6c5eb),
    ("KBBBKP", 0xb8deb82e86a71fde), ("KBBBKQ", 0xe07312b736e5afb9), ("KBBBKR", 0x92840c506fe59b0a),
    ("KBBBNK", 0x9beec5278cbdcf7c), ("KBBBPK", 0xad4d7042a8720bbe), ("KBBKBB", 0x1ea4d201f4b65fa5),
    ("KBBKBN", 0x165bb12b80d2c272), ("KBBKBP", 0x4d9cc7d65f13a043), ("KBBKNN", 0xfcd238affd905eed),
    ("KBBKNP", 0x2eee4032feb442f8), ("KBBKPP", 0x0c42e619309efe10), ("KBBNKB", 0x56b44224706c3390),
    ("KBBNKN", 0x7cb6dbab2bc38e6a), ("KBBNKP", 0x6ebfcbc280f512a9), ("KBBNKQ", 0xafb7d5f45d642c06),
    ("KBBNKR", 0x398f66f0d7781eee), ("KBBNNK", 0xab2a4f38489fb387), ("KBBNPK", 0xb29700994546fd48),
    ("KBBPKB", 0xa7d3aa3d4506047c), ("KBBPKN", 0xbafd0aeada7a3e4a), ("KBBPKP", 0x8772d72a7b12e8de),
    ("KBBPKQ", 0x134300b90812c240), ("KBBPKR", 0x311e25a76c909bdb), ("KBBPPK", 0x261896b3e104458a),
    ("KBNKBN", 0x2e78acc27433b66e), ("KBNKBP", 0x45b77e770b7fecef), ("KBNKNN", 0x50b6900311933d0a),
    ("KBNKNP", 0xab73b93b53054c1d), ("KBNKPP", 0xae34930dddbdfef8), ("KBNNKB", 0xef9b9c2aeed36b16),
    ("KBNNKN", 0x61f7b54c5dd32df2), ("KBNNKP", 0x58c0ce2b26b71843), ("KBNNKQ", 0xcc802715c8dfbec0),
    ("KBNNKR", 0x655bcaf4bee2e388), ("KBNNNK", 0x393b188459333887), ("KBNNPK", 0x47bd7b4caa9c4266),
    ("KBNPKB", 0x88731a16bcec08ad), ("KBNPKN", 0xfb5c2ce843bbc018), ("KBNPKP", 0xeb6bac1890ea30a3),
    ("KBNPKQ", 0x7b54ea8b822438cb), ("KBNPKR", 0xafdc8e001eeb737d), ("KBNPPK", 0x80ef29e34990c0d1),
    ("KBPKBP", 0xe0ec5cae101a7374), ("KBPKNN", 0x9d39e14a5949668b), ("KBPKNP", 0x126fde3ccfb41233),
    ("KBPKPP", 0xc7f16e5536aab0a3), ("KBPPKB", 0x04d7e10037dafc18), ("KBPPKN", 0x104e52a6bb762ccb),
    ("KBPPKP", 0xc7c40b428ff65d9b), ("KBPPKQ", 0x42af4e5eba539f7a), ("KBPPKR", 0x2add0e250902701e),
    ("KBPPPK", 0x89b1410752d2e640), ("KNNKNN", 0xd655e775f2424d32), ("KNNKNP", 0xc0a8ed8ae1bc832a),
    ("KNNKPP", 0xc299adf945c85c1b), ("KNNNKB", 0xf3c9859ae4c8fb7a), ("KNNNKN", 0xf1e88ebd324d0f5a),
    ("KNNNKP", 0xc3389b697329c874), ("KNNNKQ", 0xda197b63f7c78cb0), ("KNNNKR", 0x63ac3fcb6a342e31),
    ("KNNNNK", 0x02e8853fa235456f), ("KNNNPK", 0xb231a079f21b303a), ("KNNPKB", 0xdd17a33b36c58112),
    ("KNNPKN", 0xc1d978627a1978bc), ("KNNPKP", 0x2a4e85309ad0975a), ("KNNPKQ", 0xc506e5e4dae963d9),
    ("KNNPKR", 0x11642c6bd37f0687), ("KNNPPK", 0x15e26d1fd0da2081), ("KNPKNP", 0x90a05c3a5a46cf14),
    ("KNPKPP", 0x12f3f72edfa0066d), ("KNPPKB", 0xf64f73d6d2b378a4), ("KNPPKN", 0x9d62ea553f6a26d7),
    ("KNPPKP", 0x19da579c08054385), ("KNPPKQ", 0x0413cb2ad632d04f), ("KNPPKR", 0x1ca3f50693692d78),
    ("KNPPPK", 0x9cdc388f0c06db78), ("KPPKPP", 0xda75a16e9a6ab1a8), ("KPPPKB", 0xc59d1122b099479e),
    ("KPPPKN", 0xa1ab90e331c2a7f0), ("KPPPKP", 0xc3f5ca88eac90adf), ("KPPPKQ", 0xff183acd14b199e1),
    ("KPPPKR", 0x5c5c3cfef0cff849), ("KPPPPK", 0x1d97dae84f5663aa), ("KQBBBK", 0x8549212e87caa42b),
    ("KQBBKB", 0xc4a7d8c5f6d46a0c), ("KQBBKN", 0xa8591c2108a94617), ("KQBBKP", 0xd8c5de1210ec707b),
    ("KQBBKQ", 0x4a54b70f75b9bfcc), ("KQBBKR", 0xe9e8907464bfcc65), ("KQBBNK", 0xc7703245a4a41cc6),
    ("KQBBPK", 0x123aca42871221e3), ("KQBKBB", 0xa1e174f78df40c87), ("KQBKBN", 0x4d24fbf2b9f7cb55),
    ("KQBKBP", 0x6995309e35c58009), ("KQBKNN", 0x36f44044afd02bbd), ("KQBKNP", 0x4bf99d249bfd6d80),
    ("KQBKPP", 0x76abf4f1e195ea39), ("KQBKQB", 0x9f931e37192b8680), ("KQBKQN", 0x4c887fd780c31f59),
    ("KQBKQP", 0x32c15cabd63d7e76), ("KQBKRB", 0x54799f626f4d0a7c), ("KQBKRN", 0x6eda08de1dabf054),
    ("KQBKRP", 0xd324a656dd2c5b53), ("KQBKRR", 0x09855c38c26bbe40), ("KQBNKB", 0x331b634e011febe2),
    ("KQBNKN", 0xc637cf61d1da7af1), ("KQBNKP", 0x316785923ea08a31), ("KQBNKQ", 0x968bbc883f948330),
    ("KQBNKR", 0xbc64df536c3deb4c), ("KQBNNK", 0x04f8581898f3c99e), ("KQBNPK", 0x0b21c240f8e43ef2),
    ("KQBPKB", 0x27d17bf011d9d553), ("KQBPKN", 0xdbcb19c39fd5346e), ("KQBPKP", 0xbf3331939ed8fc21),
    ("KQBPKQ", 0xf4d68fcdb0223c3b), ("KQBPKR", 0x86aae47cfa6028bb), ("KQBPPK", 0x61d7cbfbd5258dde),
    ("KQNKBB", 0x94c80b8e97b5d25e), ("KQNKBN", 0x15d346d74a9f378d), ("KQNKBP", 0x50022783964adbd5),
    ("KQNKNN", 0x796d81325267fee7), ("KQNKNP", 0x0d318fb234c2288a), ("KQNKPP", 0x8052af69cac71f24),
    ("KQNKQN", 0xd12fbbc0d882ea62), ("KQNKQP", 0xbc373ed1d71e7f23), ("KQNKRB", 0x3f457ed9e296e2b8),
    ("KQNKRN", 0x44f330931f2ddd90), ("KQNKRP", 0xaf7d409ee1809a99), ("KQNKRR", 0xb8635f90a57e2bbf),
    ("KQNNKB", 0xc4460f55fe79cc7e), ("KQNNKN", 0x21529db1068b86f2), ("KQNNKP", 0x865e4da0c4f28d40),
    ("KQNNKQ", 0xf8eea41f05c10855), ("KQNNKR", 0xafae296ae36498d7), ("KQNNNK", 0x1acc0ccab72906ed),
    ("KQNNPK", 0x4ec4355c9c727bfa), ("KQNPKB", 0xf0b040e208cfa955), ("KQNPKN", 0x89d5651a3877c7f9),
    ("KQNPKP", 0x5fbc3792db107b56), ("KQNPKQ", 0x3c2e2cc4a795d528), ("KQNPKR", 0x83da7796dcd5e620),
    ("KQNPPK", 0xd0c83d24f5aeea69), ("KQPKBB", 0x9e241ee5c86be64c), ("KQPKBN", 0xfdcf39ad15f210aa),
    ("KQPKBP", 0x5072acb554e62472), ("KQPKNN", 0x4f3cfd97b69f90b4), ("KQPKNP", 0x5560b62b64dd1fab),
    ("KQPKPP", 0x0ab8674f5d178526), ("KQPKQP", 0x31b003971b7431ca), ("KQPKRB", 0x74c9006212ae423f),
    ("KQPKRN", 0x301b4c63cfbcb8f6), ("KQPKRP", 0x6a5252243b95e35d), ("KQPKRR", 0x2f284399b480db3c),
    ("KQPPKB", 0xa3be5266f43102cc), ("KQPPKN", 0x9debfa3f38937db5), ("KQPPKP", 0xa54808023a4dbd9a),
    ("KQPPKQ", 0xb159a284a3829d15), ("KQPPKR", 0xe60db846db454ba1), ("KQPPPK", 0x3881214ee4ebd236),
    ("KQQBBK", 0xc3a3423ffa5150d4), ("KQQBKB", 0x6d2b7c460ded9a80), ("KQQBKN", 0x1d01d178e5a32dc1),
    ("KQQBKP", 0xc131b4887ad5dea9), ("KQQBKQ", 0x92dff573f48dc002), ("KQQBKR", 0xbf3ec1d8a469e001),
    ("KQQBNK", 0x71a3fd03e5a666ab), ("KQQBPK", 0xda51eceee70459ad), ("KQQKBB", 0xb5f79f217c8e9a28),
    ("KQQKBN", 0x77112107126313ab), ("KQQKBP", 0xa543634a17e749d3), ("KQQKNN", 0x3cb746e654754720),
    ("KQQKNP", 0x22b014fb3c5c6651), ("KQQKPP", 0x628be33eaeb51fe7), ("KQQKQB", 0xa103fca55cf45a7b),
    ("KQQKQN", 0x333a90cd46fdbddb), ("KQQKQP", 0x0bf028a14e883764), ("KQQKQQ", 0xa96fd132a3ad55f0),
    ("KQQKQR", 0x1e7c9f27aff0af9b), ("KQQKRB", 0x8789823ff38dee8c), ("KQQKRN", 0xf82b461f70322a09),
    ("KQQKRP", 0x1eab6281cfdab2bf), ("KQQKRR", 0x985e751958ae88d6), ("KQQNKB", 0xb34c2d233cff1af0),
    ("KQQNKN", 0xb6b1fe682f0e1236), ("KQQNKP", 0x11c2351263d42add), ("KQQNKQ", 0x7a0c0cb80ccb594a),
    ("KQQNKR", 0xb48f2a457e4cd746), ("KQQNNK", 0xb87c803f3ab004dc), ("KQQNPK", 0x125ffbd95ede0182),
    ("KQQPKB", 0x0ac45ebdf6f1d4a6), ("KQQPKN", 0xe997d697ebb1db08), ("KQQPKP", 0x8801bcd200921699),
    ("KQQPKQ", 0xf0e12585ea601fe9), ("KQQPKR", 0x3fac1a9fe46aec85), ("KQQPPK", 0x03dfccb3fb7397c6),
    ("KQQQBK", 0xcfb45d4f2d5f164a), ("KQQQKB", 0x5c246763f66ebfb2), ("KQQQKN", 0x6a5f4179f4d05119),
    ("KQQQKP", 0xbb1c13b4b0045495), ("KQQQKQ", 0x4efb8b8bbf3d3b21), ("KQQQKR", 0xb48e27e174bbe312),
    ("KQQQNK", 0x7f9b62b7ccd05e1b), ("KQQQPK", 0xba0244c28563c033), ("KQQQQK", 0x934c398ed4b66243),
    ("KQQQRK", 0xac2a02bab91369a4), ("KQQRBK", 0x7e25749828f4cf91), ("KQQRKB", 0xf844627a8f28fe6f),
    ("KQQRKN", 0x2e8778bbaae76897), ("KQQRKP", 0xcb65a1acd5454093), ("KQQRKQ", 0x930a97261e05f1ca),
    ("KQQRKR", 0x3ee4362c2c387727), ("KQQRNK", 0x7b79578ef897e073), ("KQQRPK", 0xad9b946fd1fe259a),
    ("KQQRRK", 0x794cf11ca731ffd5), ("KQRBBK", 0x60d3d88c4578765c), ("KQRBKB", 0x55d84e2d22921a93),
    ("KQRBKN", 0x8809c271f36ef7e1), ("KQRBKP", 0x85644ae0cca9f635), ("KQRBKQ", 0x051d8a74c4ada750),
    ("KQRBKR", 0xb455df43f694f0c2), ("KQRBNK", 0x98aa923916a67746), ("KQRBPK", 0x87ed3bce96e8352a),
    ("KQRKBB", 0xa8772ca01b9444c9), ("KQRKBN", 0x6eed0f5dfe2e727e), ("KQRKBP", 0x4501f5566f91c53b),
    ("KQRKNN", 0x73610009dffaa6f3), ("KQRKNP", 0x9c530ce4ead0776a), ("KQRKPP", 0x755cbc2589602407),
    ("KQRKQB", 0xd718bfb1f11034b5), ("KQRKQN", 0xbd4f9cfb70651ab2), ("KQRKQP", 0x966a4041444220b1),
    ("KQRKQR", 0x7d339ef61c5ade39), ("KQRKRB", 0x46c817cd1c0caf4c), ("KQRKRN", 0x5bcbf6a8bf6bed32),
    ("KQRKRP", 0x6d44c712db7a2129), ("KQRKRR", 0x0942daf00a823235), ("KQRNKB", 0x0b817e0b27828f3c),
    ("KQRNKN", 0xdc50234b70b21beb), ("KQRNKP", 0x93430f85fd0dc429), ("KQRNKQ", 0xeee72e7c202d6add),
    ("KQRNKR", 0xca8005519bc3a74f), ("KQRNNK", 0xf4a4a4d105590aad), ("KQRNPK", 0xc7b0bb00883b69ac),
    ("KQRPKB", 0x7c3bae9c476ac9a3), ("KQRPKN", 0x1ec4bf4a461bc687), ("KQRPKP", 0x58097d309a637cbf),
    ("KQRPKQ", 0xe1456abda3509afa), ("KQRPKR", 0x96893c9348a56a6a), ("KQRPPK", 0x0f59677a1f9bd134),
    ("KQRRBK", 0xfe5cfb02dff24b00), ("KQRRKB", 0xb96a0b002232936a), ("KQRRKN", 0x2af05815d88338fe),
    ("KQRRKP", 0xca7aff6290a495ea), ("KQRRKQ", 0x5a9222bdd0ac06d5), ("KQRRKR", 0xd98e7914c761fa6b),
    ("KQRRNK", 0xd2e5b51be9a8fa1a), ("KQRRPK", 0xf197e78f9cb529f7), ("KQRRRK", 0x5bdc41cec08da505),
    ("KRBBBK", 0x4580dab56bd53270), ("KRBBKB", 0x439e542340e92389), ("KRBBKN", 0xdbf89da4a415f00d),
    ("KRBBKP", 0x1070f5d92baea060), ("KRBBKQ", 0xc7cae2225f316b79), ("KRBBKR", 0x94e3d638acbf2b25),
    ("KRBBNK", 0x397fd0f05dca0422), ("KRBBPK", 0x681cf00a46dea77b), ("KRBKBB", 0x52f79f7274cef0fb),
    ("KRBKBN", 0x2c440951d39af650), ("KRBKBP", 0x6c17aaf1d77798d2), ("KRBKNN", 0x164cc4545acfc26b),
    ("KRBKNP", 0x4e583785ae8bd226), ("KRBKPP", 0x763288738155a34c), ("KRBKRB", 0x753371abeaab9a99),
    ("KRBKRN", 0x8a53ff6cb84a30dc), ("KRBKRP", 0x097f66cb2afeba32), ("KRBNKB", 0x2141245fbf5a15e5),
    ("KRBNKN", 0x7f3878407c4098f3), ("KRBNKP", 0x58c37028e8371bb3), ("KRBNKQ", 0x7f3585bb8b0bc108),
    ("KRBNKR", 0xb32097a78041cff7), ("KRBNNK", 0xb014f9a4785c9c5f), ("KRBNPK", 0x2eee809cdef834e1),
    ("KRBPKB", 0x74807ae63f914ff7), ("KRBPKN", 0xa05a31379bf96b7f), ("KRBPKP", 0x623b7a071acf432d),
    ("KRBPKQ", 0xce1fd64d1593c7c3), ("KRBPKR", 0xba75e4637776a273), ("KRBPPK", 0x91558f465cc597d5),
    ("KRNKBB", 0xc357c55db7870e3f), ("KRNKBN", 0x8289e323a97f48b7), ("KRNKBP", 0x1628265cb2c919dc),
    ("KRNKNN", 0xdc0692824e6cf25f), ("KRNKNP", 0xe130bcc1571aad6c), ("KRNKPP", 0x605c3040c01ef6e2),
    ("KRNKRN", 0x268b92aeb54bca56), ("KRNKRP", 0x608c9d37c28b8e69), ("KRNNKB", 0x87576ba836622897),
    ("KRNNKN", 0xb3befdde5d64695c), ("KRNNKP", 0xce09a6740418b32d), ("KRNNKQ", 0x9e9ee5422e8beff6),
    ("KRNNKR", 0xb0c97abc402554d1), ("KRNNNK", 0xe47cacdda0f6f495), ("KRNNPK", 0x5ffedc78c0313cde),
    ("KRNPKB", 0x8cda04c5b7dc9d64), ("KRNPKN", 0xbe74ec6625a5e259), ("KRNPKP", 0x8fef7c8b87dadd96),
    ("KRNPKQ", 0xdd5bff7b8be8bc60), ("KRNPKR", 0xcadee1cb0b5752a8), ("KRNPPK", 0x28cf0f1daa769d2a),
    ("KRPKBB", 0xaf01de07b4fab2cb), ("KRPKBN", 0x625fa27ab3a11f21), ("KRPKBP", 0x3909bdbd74e96106),
    ("KRPKNN", 0xd143f87ff99fdf4b), ("KRPKNP", 0xe2bc8eeb6516854c), ("KRPKPP", 0x7f181a40dce84e41),
    ("KRPKRP", 0x8e4a07cc6fad7e98), ("KRPPKB", 0xe7b6e7f50fe20092), ("KRPPKN", 0x0303636cc7cac426),
    ("KRPPKP", 0x91761f046e7bf3e5), ("KRPPKQ", 0x920f65076a270d77), ("KRPPKR", 0x1d43008274152405),
    ("KRPPPK", 0x8c6f43140017a3c0), ("KRRBBK", 0xa235b013a87f10c9), ("KRRBKB", 0x15c8a5242b482bb3),
    ("KRRBKN", 0x4e7e96f05e820ddc), ("KRRBKP", 0x1e67cc75bcb873b4), ("KRRBKQ", 0x9a436ec351aeec1c),
    ("KRRBKR", 0x14e9d82fb920bba7), ("KRRBNK", 0x2420fdb41cb854d0), ("KRRBPK", 0xf4f041a91fad48f4),
    ("KRRKBB", 0x4bfd0a919151457d), ("KRRKBP", 0xf5dada4039b40a27), ("KRRKNN", 0x34abb0c5956cf6aa),
    ("KRRKNP", 0xa9fa1ab626bec5c6), ("KRRKPP", 0xa68b439869ebb0ae), ("KRRKRB", 0x401064d2a767ea37),
    ("KRRKRN", 0xeea4cb6fe39be3e6), ("KRRKRP", 0x6cac038f90de9287), ("KRRKRR", 0x531129634ec775cf),
    ("KRRNKB", 0xce8a48afe331bdc0), ("KRRNKN", 0x6fb0950cbdd8f641), ("KRRNKP", 0xe3e780d5ef41c8be),
    ("KRRNKQ", 0xdf57c050b2e66ba7), ("KRRNKR", 0xb0c9a83e3012ae1e), ("KRRNNK", 0x1bee8ec115030dc0),
    ("KRRNPK", 0xff671e68f3d3e0ef), ("KRRPKB", 0xcb554faca156e952), ("KRRPKN", 0x80a7624511dd8686),
    ("KRRPKP", 0x8f64eb021ae8d02c), ("KRRPKQ", 0x69b13b7145901f3f), ("KRRPKR", 0x1ee56d1b4d7e770a),
    ("KRRPPK", 0x3e249fa8fef014f3), ("KRRRBK", 0x69f8b544b04ff2a5), ("KRRRKB", 0x313dcd112ee8af99),
    ("KRRRKN", 0x96520d7412b1953e), ("KRRRKP", 0x44ae179504f17ee8), ("KRRRKQ", 0xead9afdf83ca3611),
    ("KRRRKR", 0x9b1fbdb86766d3b6), ("KRRRNK", 0x6388182b869735e2), ("KRRRPK", 0x42df72f4c882c353),
    ("KRRRRK", 0x0c1b685be9fa73c6),
];

/// The materials whose exact tables needed two bytes per entry (a distance
/// beyond 254 plies, or KPPKP, which the generator widened at its overflow
/// check), from the same sweep (measure.log `wide=true`): 1 five-piece and 63
/// six-piece tables. Their files take twice the memory of the others.
pub const WIDE_TABLES: &[&str] = &[
    "KPPKP", "KBBKNP", "KBBPKP", "KBBPKQ", "KBBPKR", "KBNKNP", "KBNPKP", "KBNPKQ", "KBNPKR", "KBPKNN",
    "KBPKNP", "KBPKPP", "KBPPKP", "KBPPKQ", "KBPPKR", "KNNKNP", "KNNKPP", "KNPKNP", "KNPKPP", "KNPPKP",
    "KNPPKQ", "KNPPKR", "KPPKPP", "KPPPKP", "KPPPKQ", "KQBKQP", "KQBPKP", "KQBPKQ", "KQNKPP", "KQNKQP",
    "KQNKRR", "KQNPKP", "KQNPKQ", "KQPKPP", "KQPKQP", "KQPKRB", "KQPKRR", "KQPPKP", "KQPPKQ", "KQRPKQ",
    "KRBKNN", "KRBKNP", "KRBKPP", "KRBPKP", "KRNKBB", "KRNKBN", "KRNKBP", "KRNKNN", "KRNKNP", "KRNKPP",
    "KRNPKP", "KRNPKQ", "KRPKBB", "KRPKBN", "KRPKBP", "KRPKNN", "KRPKNP", "KRPKPP", "KRPPKP", "KRPPKQ",
    "KRRKNP", "KRRKPP", "KRRPKP", "KRRPKQ",
];

/// Whether a material is recorded as needing two bytes per entry.
pub fn recorded_wide(name: &str) -> bool {
    WIDE_TABLES.contains(&name)
}

/// The recorded checksum of a table, if any.
pub fn recorded_checksum(name: &str) -> Option<u64> {
    TABLE_CHECKSUMS.iter().find(|(n, _)| *n == name).map(|&(_, c)| c)
}

/// A 64-bit hash over 8-byte little-endian words (FNV-1a style with a final
/// mix), identical to the Go implementation.
pub fn checksum(data: &[u8]) -> u64 {
    const PRIME: u64 = 0x100000001b3;
    let mut h: u64 = 0xcbf29ce484222325;
    let mut words = data.chunks_exact(8);
    for w in &mut words {
        let w = u64::from_le_bytes(w.try_into().unwrap());
        h = (h ^ w).wrapping_mul(PRIME);
        h ^= h >> 29;
    }
    for &b in words.remainder() {
        h = (h ^ b as u64).wrapping_mul(PRIME);
    }
    h ^= h >> 32;
    h = h.wrapping_mul(0x9e3779b97f4a7c15);
    h ^= h >> 29;
    h
}

/// The cache directory next to the running binary.
pub fn default_cache_dir() -> PathBuf {
    match std::env::current_exe() {
        Ok(exe) => exe.with_file_name(CACHE_DIR),
        Err(_) => PathBuf::from(CACHE_DIR),
    }
}

/// The base cache file next to the running binary.
pub fn default_path() -> PathBuf {
    default_cache_dir().join(BASE_FILE_NAME)
}

fn io_err(msg: String) -> std::io::Error {
    std::io::Error::other(msg)
}

/// Writes header + compressed values in blocks of `block_size`. The tables
/// are filled first; `step` is the resolution they all share (recorded in the
/// header).
fn write_file(path: &Path, tables: &[&Table], raw_checksum: u64, step: u32, block_size: usize, workers: usize) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // written beside the target and renamed when complete: a reader never
    // sees a partial file (a sweep may be killed while writing)
    let tmp = tmp_path(path);
    let mut w = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(&tmp)?);
    write_to(&mut w, tables, raw_checksum, step, block_size, workers)?;
    use std::io::Write;
    w.flush()?;
    drop(w);
    std::fs::rename(&tmp, path)
}

/// Writes a file that was packed into memory before (`Set::pack_raw_table`),
/// through its temporary twin like `write_file`: the sweep hands this to a
/// thread of its own, so the next material is computed meanwhile.
pub fn write_packed(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = tmp_path(path);
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

/// The content of a cache file: header and compressed values.
fn write_to<W: std::io::Write>(w: &mut W, tables: &[&Table], raw_checksum: u64, step: u32, block_size: usize, workers: usize) -> std::io::Result<()> {
    for t in tables {
        if !t.is_generated() {
            return Err(io_err(format!("egtb: table {} not generated", t.mat.name())));
        }
        if t.step() != step {
            return Err(io_err(format!("egtb: table {} has step {}, the file needs {}", t.mat.name(), t.step(), step)));
        }
        t.fill();
    }
    let wide = tables.len() == 1 && tables[0].is_wide();
    // a single table (every material beyond the base) is packed in place: no
    // copy of 15 GB for a six-piece table; only the base file concatenates
    let joined: Vec<u8>;
    let raw: &[u8] = if tables.len() == 1 {
        tables[0].bytes()
    } else {
        let size: usize = tables.iter().map(|t| t.size).sum();
        let mut v = Vec::with_capacity(size);
        for t in tables {
            v.extend_from_slice(t.bytes());
        }
        joined = v;
        &joined
    };
    w.write_all(MAGIC)?;
    w.write_all(&[step as u8, if wide { 2 } else { 1 }, 0, 0])?;
    w.write_all(&raw_checksum.to_le_bytes())?;
    if wide {
        // the two planes, built block by block from the entries
        let v = tables[0].wide_values();
        let n = v.len();
        lz::pack_indexed_with(w, 2 * n, block_size, workers, &|off, dst| {
            for (j, b) in dst.iter_mut().enumerate() {
                let pos = off + j;
                *b = if pos < n { Value(v[pos]).to_planes().0 } else { Value(v[pos - n]).to_planes().1 };
            }
        })?;
    } else {
        lz::pack_indexed_to(w, raw, block_size, workers)?;
    }
    Ok(())
}

/// The temporary twin a file is written to before it is renamed into place
/// ("5-KBNKQ.dtm.tmp"): a reader never sees a partial file.
pub fn tmp_path(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(".tmp");
    PathBuf::from(p)
}

/// Opens a cache file and reads its header and block index; the caller either
/// unpacks everything (`read_file`) or keeps the file for block reads
/// (`open_table`). `size` is the number of entries the tables expect.
/// Returns the file, the index, the step, the raw checksum and whether the
/// entries are wide (two planes).
fn open_file(path: &Path, size: usize) -> std::io::Result<(std::fs::File, lz::Index, u32, u64, bool)> {
    let f = std::fs::File::open(path)?;
    let mut header = [0u8; HEADER_BYTES];
    if lz::read_exact_at(&f, &mut header, 0).is_err() || &header[..4] != MAGIC {
        return Err(io_err(format!("egtb: {} is not a cache file (format {:?}, regenerate)", path.display(), String::from_utf8_lossy(&header[..4]))));
    }
    let step = (header[4] as u32).max(1);
    let wide = match header[5] {
        0 | 1 => false,
        2 => true,
        w => return Err(io_err(format!("egtb: {} has {} bytes per entry", path.display(), w))),
    };
    let raw_checksum = u64::from_le_bytes(header[8..16].try_into().unwrap());
    let ix = lz::Index::read(&f, HEADER_BYTES as u64).map_err(|e| io_err(format!("egtb: {}: {}", path.display(), e)))?;
    let want = if wide { 2 * size } else { size };
    if ix.raw_len != want {
        return Err(io_err(format!("egtb: {} holds {} bytes, expected {}", path.display(), ix.raw_len, want)));
    }
    Ok((f, ix, step, raw_checksum, wide))
}

/// Reads a cache file into the tables as a whole (the base). Returns the raw
/// checksum from the header; the entries are filled (dead and illegal ones
/// hold their predecessor) and the tables carry the file's step.
fn read_file(path: &Path, tables: &mut [&mut Table], workers: usize) -> std::io::Result<u64> {
    let size: usize = tables.iter().map(|t| t.size).sum();
    for t in tables.iter_mut() {
        t.clear();
    }
    let (f, ix, step, raw_checksum, wide) = open_file(path, size)?;
    if wide && tables.len() != 1 {
        return Err(io_err(format!("egtb: {} is wide, only single tables can be", path.display())));
    }
    for t in tables.iter_mut() {
        t.allocate(wide);
    }
    // decompress into one buffer, then split (the tables are separate slices)
    if wide {
        // each plane block ors its byte into the entries (low and high
        // byte), then every entry turns from the plane form into its value
        let t = &tables[0];
        let n = t.size;
        ix.unpack_each(&f, workers, &|off, data| {
            for (j, &b) in data.iter().enumerate() {
                let pos = off + j;
                if pos < n {
                    t.wide[pos].fetch_or(b as u16, Ordering::Relaxed);
                } else {
                    t.wide[pos - n].fetch_or((b as u16) << 8, Ordering::Relaxed);
                }
            }
        })
        .map_err(|e| io_err(format!("egtb: {}: {}", path.display(), e)))?;
        let chunks = t.wide.chunks(1 << 20).collect::<Vec<_>>();
        let next = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|s| {
            for _ in 0..workers.max(1) {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(c) = chunks.get(i) else { break };
                    for e in c.iter() {
                        let raw = e.load(Ordering::Relaxed);
                        e.store(Value::from_planes(raw as u8, (raw >> 8) as u8).0, Ordering::Relaxed);
                    }
                });
            }
        });
    } else if tables.len() == 1 {
        ix.unpack_all(&f, tables[0].bytes_mut(), workers).map_err(|e| io_err(format!("egtb: {}: {}", path.display(), e)))?;
    } else {
        let mut dst = vec![0u8; size];
        ix.unpack_all(&f, &mut dst, workers).map_err(|e| io_err(format!("egtb: {}: {}", path.display(), e)))?;
        let mut off = 0;
        for t in tables.iter_mut() {
            let n = t.size;
            t.bytes_mut().copy_from_slice(&dst[off..off + n]);
            off += n;
        }
    }
    for t in tables.iter_mut() {
        t.set_filled(true);
        t.set_step(step);
    }
    Ok(raw_checksum)
}

/// Opens a cache file for block reads: the table stays compressed in the
/// file, a block cache (`CACHE_BYTES`) serves the lookups. Returns the raw
/// checksum from the header.
fn open_table(path: &Path, t: &mut Table) -> std::io::Result<u64> {
    t.clear();
    let (f, ix, step, raw_checksum, wide) = open_file(path, t.size)?;
    t.set_packed(super::blockstore::BlockStore::new(f, ix, if wide { t.size } else { 0 }));
    t.set_filled(true);
    t.set_step(step);
    Ok(raw_checksum)
}

impl Set {
    /// Checksum of the base tables in file order (they must be complete): a
    /// hash of the per-table hashes keeps the tables independent.
    pub fn checksum(&self) -> u64 {
        let mut h: u64 = 0x9e3779b97f4a7c15;
        for t in self.base() {
            h = (h ^ t.raw_checksum()).wrapping_mul(0x100000001b3);
            h ^= h >> 31;
        }
        h
    }

    /// The cache file of a material beyond the base, e.g. "dtm/5-KBNKQ.bin".
    pub fn table_path(&self, t: &Table) -> PathBuf {
        self.dtm_path().join(format!("{}-{}.bin", t.mat.pieces(), t.mat.name()))
    }

    /// The folder of the distance tables beyond the base.
    pub fn dtm_path(&self) -> PathBuf {
        self.cache_dir.join(DTM_DIR)
    }

    /// The folder of our own WDL tables (syzygy::write).
    pub fn wdl_path(&self) -> PathBuf {
        self.cache_dir.join(WDL_DIR)
    }

    /// The own WDL table of a material, e.g. "wdl/5-KBNKQ.wdl".
    pub fn wdl_file(&self, m: &Material) -> PathBuf {
        self.wdl_path().join(format!("{}-{}.wdl", m.pieces(), m.name()))
    }

    /// The own DTM table of a material, e.g. "dtm/5-KBNKQ.dtm".
    pub fn dtm_file(&self, m: &Material) -> PathBuf {
        self.dtm_path().join(format!("{}-{}.dtm", m.pieces(), m.name()))
    }

    /// Drops the values of a table beyond the base from memory (and closes
    /// its file, when it was loaded packed); the table stays registered and
    /// can be loaded or generated again.
    pub fn release(&mut self, ti: usize) {
        self.tables[ti].clear();
    }

    /// The exact cache file of a material beyond the base ("5-KBNKQ.bin.raw",
    /// `RAW_SUFFIX`).
    pub fn raw_path(&self, t: &Table) -> PathBuf {
        let mut p = self.table_path(t).into_os_string();
        p.push(RAW_SUFFIX);
        PathBuf::from(p)
    }

    /// Reads the material of a cache file name as `table_path` writes it
    /// ("5-KRRKN.bin", or its exact twin "5-KRRKN.bin.raw"); None for any
    /// other name, the base file included.
    pub fn parse_file_name(name: &str) -> Option<Material> {
        let name = name.strip_suffix(RAW_SUFFIX).unwrap_or(name);
        let base = name.strip_suffix(".bin")?;
        let (_, mat_name) = base.split_once('-')?;
        let m = Material::parse(mat_name).ok()?;
        if format!("{}-{}.bin", m.pieces(), m.name()) != name {
            return None;
        }
        Some(m)
    }

    /// The base cache file in the set's cache directory.
    pub fn base_path(&self) -> PathBuf {
        self.cache_dir.join(BASE_FILE_NAME)
    }

    fn clear_base(&mut self) {
        let n = self.base().len();
        for t in &mut self.tables[..n] {
            t.clear();
        }
    }

    /// Writes the base tables to path (compressed).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tables: Vec<&Table> = self.base().iter().collect();
        write_file(path, &tables, self.checksum(), 1, lz::BLOCK_SIZE, IO_WORKERS)
    }

    /// Reads the base tables from path and checks size and checksum.
    pub fn load(&mut self, path: &Path) -> std::io::Result<()> {
        let n = self.base().len();
        let result = {
            let mut tables: Vec<&mut Table> = self.tables[..n].iter_mut().collect();
            read_file(path, &mut tables, IO_WORKERS)
        };
        let sum = match result {
            Ok(r) => r,
            Err(e) => {
                self.clear_base();
                return Err(e);
            }
        };
        if !self.tables[0].is_exact() {
            let step = self.tables[0].step();
            self.clear_base();
            return Err(io_err(format!("egtb: {} is quantised (step {}), the base must be exact", path.display(), step)));
        }
        // the header carries the combined checksum of the generator run; the
        // single table checksums cannot be recomputed from filled values, so
        // the recorded constants stand in for them
        for t in &self.tables[..n] {
            t.raw_checksum.store(recorded_checksum(&t.mat.name()).unwrap_or(0), std::sync::atomic::Ordering::Relaxed);
        }
        if FILE_CHECKSUM != 0 && sum != FILE_CHECKSUM {
            self.clear_base();
            return Err(io_err(format!("egtb: {} has checksum {:016x}, expected {:016x}", path.display(), sum, FILE_CHECKSUM)));
        }
        Ok(())
    }

    /// Writes one table (a material beyond the base) to path (compressed). The
    /// table is quantised to `FILE_STEP` **in place** first (no copy of 15 GB
    /// for a six-piece table): afterwards it is what a reload gives, the search
    /// resolves its bounds, and it can no longer serve the generator as a
    /// dependency. The header keeps the checksum of the exact values. Verify
    /// before saving.
    pub fn save_table(&self, ti: usize, path: &Path) -> std::io::Result<()> {
        let t = &self.tables[ti];
        if !t.is_generated() {
            return Err(io_err(format!("egtb: table {} not generated", t.mat.name())));
        }
        let sum = t.raw_checksum(); // of the exact values, before quantising
        if t.is_exact() {
            t.quantize(FILE_STEP);
        }
        write_file(path, &[t], sum, t.step(), BLOCK_BYTES, IO_WORKERS)
    }

    /// Writes one exact table (a material beyond the base) to path
    /// unquantised (`raw_path`): the values stay exact in memory, the file
    /// can serve a later generation as a dependency. The table must still be
    /// exact.
    pub fn save_raw_table(&self, ti: usize, path: &Path) -> std::io::Result<()> {
        self.save_raw_table_with(ti, path, IO_WORKERS)
    }

    /// The exact file of a table (`save_raw_table`) packed into memory on
    /// `workers` threads, for `write_packed`: only the compressed bytes.
    pub fn pack_raw_table(&self, ti: usize, workers: usize) -> std::io::Result<Vec<u8>> {
        let t = &self.tables[ti];
        if !t.is_exact() {
            return Err(io_err(format!("egtb: table {} is quantised (step {}), no exact file", t.mat.name(), t.step())));
        }
        let mut v = Vec::new();
        write_to(&mut v, &[t], t.raw_checksum(), 1, BLOCK_BYTES, workers)?;
        Ok(v)
    }

    /// `save_raw_table` packed on `workers` threads.
    pub fn save_raw_table_with(&self, ti: usize, path: &Path, workers: usize) -> std::io::Result<()> {
        let t = &self.tables[ti];
        if !t.is_generated() {
            return Err(io_err(format!("egtb: table {} not generated", t.mat.name())));
        }
        if !t.is_exact() {
            return Err(io_err(format!("egtb: table {} is quantised (step {}), no exact file", t.mat.name(), t.step())));
        }
        write_file(path, &[t], t.raw_checksum(), 1, BLOCK_BYTES, workers)
    }

    /// Loads an exact table (`raw_path`) compressed into memory
    /// (`MemStore`): the generator looks its values up block by block. Checks
    /// the step, the size and, when recorded and `check` is set (as in
    /// `load_raw_table_checked`), the checksum from the header.
    pub fn load_raw_packed(&mut self, ti: usize, path: &Path, check: bool) -> std::io::Result<()> {
        let t = &mut self.tables[ti];
        t.clear();
        let (f, ix, step, sum, wide) = open_file(path, t.size)?;
        if step != 1 {
            return Err(io_err(format!("egtb: {} is quantised (step {}), expected an exact table", path.display(), step)));
        }
        if let Some(want) = recorded_checksum(&t.mat.name()).filter(|_| check) {
            if sum != want {
                return Err(io_err(format!("egtb: {} has checksum {:016x}, expected {:016x}", path.display(), sum, want)));
            }
        }
        t.set_mem(super::blockstore::MemStore::new(&f, ix, if wide { t.size } else { 0 })?);
        t.raw_checksum.store(sum, std::sync::atomic::Ordering::Relaxed);
        t.set_filled(true);
        Ok(())
    }

    /// The bytes the exact file of a table takes (what `load_raw_packed`
    /// holds in memory, a few KB of header and index more); 0 without a file.
    pub fn raw_file_bytes(&self, t: &Table) -> usize {
        std::fs::metadata(self.raw_path(t)).map_or(0, |m| m.len() as usize)
    }

    /// Whether the exact file of a table holds two bytes per entry (header
    /// byte 5), so that loading it takes twice its size; without a file
    /// whether the material is recorded as wide (`WIDE_TABLES`). Reads the
    /// header only.
    pub fn raw_is_wide(&self, t: &Table) -> bool {
        let Ok(f) = std::fs::File::open(self.raw_path(t)) else { return recorded_wide(&t.mat.name()) };
        let mut header = [0u8; HEADER_BYTES];
        lz::read_exact_at(&f, &mut header, 0).is_ok() && &header[..4] == MAGIC && header[5] == 2
    }

    /// Reads an exact table (`raw_path`) completely into memory, as a
    /// generation needs its dependencies. Checks the step, the size and, when
    /// recorded, the checksum.
    pub fn load_raw_table(&mut self, ti: usize, path: &Path) -> std::io::Result<()> {
        self.load_raw_table_checked(ti, path, true)
    }

    /// `load_raw_table`, comparing with the recorded checksum only when
    /// `check` is set (a test widens a recorded one-byte table on purpose).
    pub fn load_raw_table_checked(&mut self, ti: usize, path: &Path, check: bool) -> std::io::Result<()> {
        let result = read_file(path, &mut [&mut self.tables[ti]], IO_WORKERS);
        let t = &mut self.tables[ti];
        let sum = match result {
            Ok(r) => r,
            Err(e) => {
                t.clear();
                return Err(e);
            }
        };
        if !t.is_exact() {
            let step = t.step();
            t.clear();
            return Err(io_err(format!("egtb: {} is quantised (step {}), expected an exact table", path.display(), step)));
        }
        t.raw_checksum.store(sum, std::sync::atomic::Ordering::Relaxed);
        if let Some(want) = recorded_checksum(&t.mat.name()).filter(|_| check) {
            if sum != want {
                t.clear();
                return Err(io_err(format!("egtb: {} has checksum {:016x}, expected {:016x}", path.display(), sum, want)));
            }
        }
        Ok(())
    }

    /// Writes the exact file (`raw_path`) and the quantised file
    /// (`table_path`) of a table that is exact in memory, each only when
    /// missing, and leaves the table exact: `save_table` quantises in place,
    /// so the exact file is loaded back afterwards (0.4 s for a five-piece
    /// table). For a background job's dependencies, which later tables of the
    /// job still need exactly. `progress` gets a line per written file.
    /// Returns the first error.
    pub fn save_both(&mut self, ti: usize, progress: Progress) -> std::io::Result<()> {
        let (raw, quantised) = (self.raw_path(&self.tables[ti]), self.table_path(&self.tables[ti]));
        if !self.tables[ti].is_exact() {
            return Err(io_err(format!("egtb: table {} is quantised (step {}), no exact file", self.tables[ti].mat.name(), self.tables[ti].step())));
        }
        if !raw.exists() {
            self.save_raw_table(ti, &raw)?;
            progress(&format!("egtb: written exact to {}", raw.display()));
        }
        if !quantised.exists() {
            self.save_table(ti, &quantised)?;
            progress(&format!("egtb: written to {}", quantised.display()));
            self.load_raw_table(ti, &raw)?;
        }
        Ok(())
    }

    /// Opens one table (a material beyond the base) from path: it stays
    /// compressed in the file and is read block by block (`open_table`).
    /// Checks size and, when recorded, the checksum.
    pub fn load_table(&mut self, ti: usize, path: &Path) -> std::io::Result<()> {
        self.load_table_checked(ti, path, true)
    }

    /// `load_table`, comparing with the recorded checksum only when `check`
    /// is set.
    pub fn load_table_checked(&mut self, ti: usize, path: &Path, check: bool) -> std::io::Result<()> {
        let result = open_table(path, &mut self.tables[ti]);
        let t = &mut self.tables[ti];
        let sum = match result {
            Ok(r) => r,
            Err(e) => {
                t.clear();
                return Err(e);
            }
        };
        t.raw_checksum.store(sum, std::sync::atomic::Ordering::Relaxed);
        if let Some(want) = recorded_checksum(&t.mat.name()).filter(|_| check) {
            if sum != want {
                t.clear();
                return Err(io_err(format!("egtb: {} has checksum {:016x}, expected {:016x}", path.display(), sum, want)));
            }
        }
        Ok(())
    }

    /// The complete base set: from the cache file when it is present and
    /// correct, otherwise freshly generated, checked against the constants and
    /// written.
    pub fn load_or_generate(path: &Path, workers: usize, write: bool, progress: Progress) -> Set {
        let mut s = Set::new();
        if let Some(dir) = path.parent() {
            s.cache_dir = dir.to_path_buf();
        }
        let start = Instant::now();
        match s.load(path) {
            Ok(()) => {
                progress(&format!("egtb: loaded {} MB from {} in {:.2} s", s.base_size() >> 20, path.display(), start.elapsed().as_secs_f64()));
                return s;
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => progress(&format!("egtb: {}", e)),
            Err(_) => {}
        }
        let (mut done, total) = (0, s.base().len());
        s.generate_all(workers, &mut |line| {
            if line.contains(" wins ") {
                done += 1; // one summary line per table, no levels
                progress(&format!("({} / {}) {}", done, total, line));
            }
        });
        progress(&format!("egtb: generated {} tables in {:.1} s", s.base().len(), start.elapsed().as_secs_f64()));
        s.check_checksums(progress);
        if !write {
            for t in s.base() {
                t.fill();
            }
            return s;
        }
        match s.save(path) {
            Ok(()) => progress(&format!("egtb: written to {}", path.display())),
            Err(e) => progress(&format!("egtb: {}", e)),
        }
        s
    }

    /// The table of a material beyond the base: from its cache file when
    /// present and correct, otherwise generated (with its dependencies) and
    /// written. Returns (table index, stats, loaded); the stats are only
    /// meaningful after a generation.
    pub fn load_or_generate_table(&mut self, m: Material, workers: usize, progress: Progress) -> (usize, Stats, bool) {
        let ti = self.add_material(m);
        if self.tables[ti].is_generated() {
            return (ti, Stats::default(), true);
        }
        let path = self.table_path(&self.tables[ti]);
        match self.load_table(ti, &path) {
            Ok(()) => {
                progress(&format!("egtb: loaded {} ({} MB)", path.display(), self.tables[ti].size >> 20));
                return (ti, Stats::default(), true);
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => progress(&format!("egtb: {}", e)),
            Err(_) => {}
        }
        let st = self.generate(ti, workers, progress);
        match self.save_table(ti, &path) {
            Ok(()) => progress(&format!("egtb: written to {}", path.display())),
            Err(e) => progress(&format!("egtb: {}", e)),
        }
        (ti, st, false)
    }

    /// Compares every base table and the whole base with the recorded constants
    /// and reports differences (or the values to record, when missing). Returns
    /// the number of mismatches.
    pub fn check_checksums(&self, progress: Progress) -> usize {
        let mut bad = 0;
        for t in self.base() {
            let sum = t.raw_checksum();
            let name = t.mat.name();
            match recorded_checksum(&name) {
                None => progress(&format!("  (\"{}\", 0x{:016x}),", name, sum)),
                Some(want) if sum != want => {
                    bad += 1;
                    progress(&format!("egtb: {} checksum {:016x}, expected {:016x}", name, sum, want));
                }
                _ => {}
            }
        }
        let sum = self.checksum();
        if FILE_CHECKSUM == 0 {
            progress(&format!("egtb: file checksum 0x{:016x} (record it as FILE_CHECKSUM)", sum));
        } else if sum != FILE_CHECKSUM {
            bad += 1;
            progress(&format!("egtb: file checksum {:016x}, expected {:016x}", sum, FILE_CHECKSUM));
        } else {
            progress("egtb: checksums ok");
        }
        bad
    }
}

