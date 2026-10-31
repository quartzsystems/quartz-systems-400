# Media: tape, optical, image catalogs

Save and restore write to three classes of media: tape (real drives and
media libraries, and virtual volumes in image catalogs), optical, and the
image catalogs that hold virtual volumes as stream files. This document
specifies the on-disk formats and the models, and how the storage
service wires them to the devices and the commands ("In the storage
service" at the end).

The tape class is the `media/tape` crate. The optical part and the image
catalog part have their own sections below.

## Tape: overview

One label-and-command layer sits over a transport trait:

```text
INZTAP CHKTAP DUPTAP DSPTAP CPYTOTAP/CPYFRMTAP SAV*/RST* (callers)
        |
volume:: inztap, chktap, duptap, labels, FileWriter, FileReader
        |  (VolumeProvider for the next volume)
label::  VOL1, HDR1/2, EOF1/2, EOV1/2 in CP037 or ASCII
        |
TapeTransport  ------------------------------  Changer
  VirtualTape (one volume file)                 VirtualLibrary
  VirtualLibrary (its selected drive)           SgChanger (SG_IO)
  StTransport (Linux st)
```

`device::` holds the data model the commands show: TAPnn and TAPMLBnn
resource names, and the *TAPMLB cartridge model (categories, ADDTAPCTG,
RMVTAPCTG, CHGTAPCTG, VOL selection, drive allocation).

## Tape: transports

`TapeTransport` is one drive. Every operation follows the Linux `st`
driver (Documentation/scsi/st.rst), so the physical and virtual
transports behave the same:

| Operation | Meaning |
|-----------|---------|
| `rewind` | to the beginning of tape (BOT) |
| `fsf(n)` | forward over `n` tape marks; stops just past the last |
| `bsf(n)` | backward over `n` tape marks; stops just before (BOT side of) the last |
| `fsr(n)`, `bsr(n)` | over `n` blocks; a tape mark stops them with `TapeMark` |
| `eom` | to the end of recorded data |
| `write_eof(n)` | `n` tape marks; everything after the position is lost |
| `erase` | from the position to the end of the medium |
| `unload` | rewind and go offline (MTOFFL) |
| `set_block_size(n)` | fixed blocks of `n` bytes, or 0 for variable |
| `set_density(name)` | the format for writes from BOT |
| `status` | loaded, BOT, EOT (early warning), EOD, write protected, density, block size, file and block number |
| `read_block(buf)` | one block's length, `TapeMark` (consumed) or `EndOfData` |
| `write_block(data)` | one block of 1 to 262144 bytes; reports the early warning |

A write past the early warning point succeeds and says so; the label
layer then ends the volume. A write with no room left fails with
`EndOfMedium` and records nothing.

`Changer` is a library's robot: `move_medium(from, to)` and
`read_element_status()` (every transport, slot, import/export and drive
element, full or empty, with its volume tag and source slot).

### Linux st and sg

`StTransport` runs over `StIoctl` (MTIOCTOP, MTIOCGET, MTIOCPOS, `read`,
`write` on `/dev/nstN`); `SgChanger` over `SgIoctl` (SG_IO on the
changer's `/dev/sgN`). The real calls are `media/tape/src/st/raw.rs`
(Linux only, the crate's one `unsafe` file). The tests replay the
hand-written transcripts in `media/tape/tests/fixtures/`.

How `st` results are read:

| Result | Meaning |
|--------|---------|
| `read` returns 0, GMT_EOD clear | tape mark |
| `read` returns 0 or EIO, GMT_EOD set | end of data |
| `read` fails ENOMEM | block longer than the buffer |
| spacing fails EIO | GMT_EOD: end of data; GMT_BOT: beginning of tape; GMT_EOF (records): tape mark |
| `write` fails ENOSPC, GMT_EOT set | early warning: the block is written again once and reported with the warning |
| a second ENOSPC, or EIO after the warning | end of medium |
| EROFS or EACCES | write protected |
| ENOMEDIUM | no tape |

The early-warning handling assumes the driver accepts writes after the
first ENOSPC (it does for trailer labels); the physical acceptance below
checks it against real hardware.

READ ELEMENT STATUS asks for all element types with primary volume tags
(VOLTAG), from address 0, up to 65535 elements, 64 KiB of data. The
reply is parsed by each page's descriptor length, so drives that add
identification descriptors (52-byte descriptors) and drives that do not
(48 bytes) both work. MOVE MEDIUM uses the transport element the last
status reported. Sense keys map: 5/3B/0E source empty, 5/3B/0D
destination full, 5/21/01 bad element address; anything else is a check
condition with its codes.

Density codes shown as IBM i format names: 40h *ULTRIUM1, 42h
*ULTRIUM2, 44h *ULTRIUM3, 46h *ULTRIUM4, 58h *ULTRIUM5, 5Ah *ULTRIUM6,
5Ch *ULTRIUM7, 5Eh *ULTRIUM8, 60h *ULTRIUM9; 0 is *DEVTYPE; others show
as X'nn'.

## Tape: virtual volume format

A virtual tape volume is one file (an image catalog entry; any
`Read + Write + Seek` byte store). All integers are little-endian;
checksums are CRC-32C (Castagnoli). The file is a header and then the
data area, a chain of frames.

### `volume header` (512 bytes)

| Offset | Size | Field | Contents |
|--------|------|-------|----------|
| 0 | 8 | magic | `QSTAPVOL` |
| 8 | 2 | version | format version, 1 |
| 10 | 2 | flags | bit 0: write protected |
| 12 | 4 | header_len | 512; the data area starts here |
| 16 | 8 | capacity | bytes in the data area, frame headers included (IMGSIZ megabytes x 1048576) |
| 24 | 8 | early_warning | data-area offset past which writes report early warning |
| 32 | 16 | density | density name, ASCII, space padded (`*VRT256K`) |
| 48 | 6 | volume_tag | the cartridge's tag (its catalog volume name), ASCII, space padded |
| 54 | 454 | reserved | zeros |
| 508 | 4 | crc32c | over bytes 0 to 507 |

The early warning point is `capacity - reserve`, where the reserve is
1/32 of the capacity, at least four maximum blocks with their frame
headers (1048640 bytes), and never more than half the capacity.

### `frame header` (16 bytes)

| Offset | Size | Field | Contents |
|--------|------|-------|----------|
| 0 | 1 | kind | 1 data block, 2 tape mark |
| 1 | 3 | reserved | zeros |
| 4 | 4 | length | payload bytes: 1 to 262144 for a block, 0 for a mark |
| 8 | 4 | epoch | write epoch |
| 12 | 4 | crc32c | over bytes 0 to 11 and the payload |

The payload follows its header directly; the next frame follows the
payload. There is no padding.

Rules:

- **End of data.** Opening a volume walks the frames from offset 0 of the
  data area. The chain ends at the first frame that is cut short by the
  end of the file, has an invalid kind or length, fails its checksum,
  ends past the capacity, or has a smaller epoch than the frame before
  it. The end of data is the end of the last frame in the chain. A torn
  last block is therefore not there, and the block before it is the last
  one.
- **Epochs.** The first write after a volume is opened, and every write
  that does not start at the end of data, takes a new epoch one higher
  than any in the chain. Frames left beyond the new end of data from an
  older write carry smaller epochs, so they can never rejoin the chain.
- **Writes.** A block or tape mark is written at the position, and
  everything after it is gone. A frame that would end past the capacity
  is not written (end of medium). A write that ends past the early
  warning point reports the warning.
- **Creating.** A new volume is the header followed by 16 zero bytes (an
  invalid frame), so stale bytes of a reused file are never read.
- **Erase** zeroes the data area from the position to the old end of
  data.
- **Write protection** is the header flag; changing it rewrites the
  header.

`VirtualLibrary` holds volumes in storage slots, import/export stations
and drives at fixed element addresses: transport 0, drives from 0x0100,
stations from 0x0200, slots from 0x0400. As a `TapeTransport` the
library is its selected drive. A cartridge cannot leave a drive that
still has it loaded (sense 5/53/02), as with a real changer: unload
first.

## Tape: labels

IBM standard labels are 80-byte blocks. A labeled volume is

```text
VOL1 HDR1 HDR2 TM data... TM EOF1 EOF2 TM [HDR1 HDR2 TM ... EOF2 TM]... TM
```

so the file at index `k` on the volume starts after `3k` tape marks, and
two tape marks in a row end the volume. A newly initialized volume is
`VOL1 TM TM`. An unlabeled (*NL) volume is data files each ended by a
tape mark, and one more tape mark at the end; NEWVOL(*NONE) writes just
`TM TM`.

Labels are written in CP037 (CODE(*EBCDIC), as IBM i writes them) or
ASCII (CODE(*ASCII)), with the same layout. Reading finds the code from
the VOL1 identifier. Numbers are zero-padded decimal; text is padded with
spaces. Offsets below count from 0.

### `VOL1` (80 bytes)

| Offset | Size | Field | Contents |
|--------|------|-------|----------|
| 0 | 3 | label_id | `VOL` |
| 3 | 1 | label_number | `1` |
| 4 | 6 | volume_id | NEWVOL |
| 10 | 1 | accessibility | blank |
| 11 | 26 | reserved_1 | blanks |
| 37 | 14 | owner_id | NEWOWNID |
| 51 | 29 | reserved_2 | blanks |

### `HDR1` (80 bytes)

EOF1 and EOV1 have the same layout.

| Offset | Size | Field | Contents |
|--------|------|-------|----------|
| 0 | 3 | label_id | `HDR`, `EOF` or `EOV` |
| 3 | 1 | label_number | `1` |
| 4 | 17 | file_id | the file label (FILELABEL) |
| 21 | 6 | file_set_id | volume identifier of the file's first volume |
| 27 | 4 | volume_seq | volume sequence number within the file, from 0001 |
| 31 | 4 | file_seq | file sequence number (SEQNBR) |
| 35 | 4 | generation | generation number |
| 39 | 2 | version | generation version |
| 41 | 6 | creation_date | `cyyddd` |
| 47 | 6 | expiration_date | `cyyddd`; ` 00000` none, ` 99365` *PERM |
| 53 | 1 | security | `0` |
| 54 | 6 | block_count | data blocks on this volume, low six digits (zeros in HDR1) |
| 60 | 13 | system_code | `QS/400` |
| 73 | 3 | reserved | blanks |
| 76 | 4 | block_count_high | block count, high four digits |

`cyyddd`: `c` is blank for 19xx, `0` for 20xx, `1` for 21xx; `yy` the
year in the century, `ddd` the day of the year. On reading, day 365, 366
or 999 of year 99 in any century is *PERM. A file is active (protected)
while its expiration date is later than today.

### `HDR2` (80 bytes)

EOF2 and EOV2 have the same layout.

| Offset | Size | Field | Contents |
|--------|------|-------|----------|
| 0 | 3 | label_id | `HDR`, `EOF` or `EOV` |
| 3 | 1 | label_number | `2` |
| 4 | 1 | record_format | `F`, `V` or `U` |
| 5 | 5 | block_length | block length; `00000` when over 99999 |
| 10 | 5 | record_length | record length |
| 15 | 1 | density | blank |
| 16 | 1 | volume_switch | `1` on a continuation volume, else `0` |
| 17 | 17 | job_step | the job that wrote the file |
| 34 | 2 | recording | blanks |
| 36 | 1 | control_char | blank |
| 37 | 1 | reserved_1 | blank |
| 38 | 1 | block_attribute | `B` for F and V, blank for U |
| 39 | 31 | reserved_2 | blanks |
| 70 | 10 | large_block_length | block length when over 99999, else blanks |

Record formats for the data part of CPYTOTAP and CPYFRMTAP: F pads
records to the record length and blocks as many as fit; V puts a 4-byte
block descriptor and 4-byte record descriptors (big-endian lengths that
include the descriptor) around the records, the record length counting
the descriptor; U is one record per block.

### Operations

- **INZTAP**: NEWVOL (or *NONE for unlabeled), NEWOWNID, VOL (the volume
  that must be mounted), CHECK(*YES) refuses while any file is active,
  CHECK(*FIRST) only looks at the first, CHECK(*NO) does not read the
  volume (VOL is not checked either). DENSITY, CODE, CLEAR(*YES) erases
  the whole volume first, ENDOPT.
- **Writing a file** at SEQNBR(n) overwrites file `n` and loses every
  file after it, so any of them still active refuses the write (unless
  the operator allowed it); SEQNBR(*END) writes after the last complete
  file; a number past that is refused.
- **Reading a file** finds it by SEQNBR, FILELABEL or both.
- **DSPTAP DATA(*LABELS)** lists every file's header labels with the
  block count of its trailer, and whether it ends with EOF or EOV.
- **CHKTAP**: VOL, SEQNBR (number, *END, or *ONLY: no sequence number),
  FILELABEL; with ENDOPT(*LEAVE) the drive stays at the file's HDR1.
- **DUPTAP**: FROMSEQNBR (*ALL or a range), TOSEQNBR (*FROMSEQ, *END or a
  starting number), FROMVOL, TOVOL. Labels are copied (the file set
  identifier becomes the target volume's); the target keeps its VOL1.
  Unlabeled volumes duplicate to unlabeled volumes.
- **ENDOPT**: *REWIND, *LEAVE (writing a file leaves the drive between
  the final two tape marks), *UNLOAD.

## Tape: multi-volume files

When a data block of a labeled file is written past the early warning
point (or does not fit at all), the volume ends with
`TM EOV1 EOV2 TM TM`, carrying this volume's block count. The caller's
`VolumeProvider` is asked for the next volume with a `VolumeRequest`:
the volume sequence number wanted, the volume from the VOL list (or none
for VOL(*MOUNTED) or when the list is used up), the volume just finished,
the file label and sequence number, and whether the file is being
written. The caller turns that into an inquiry to QSYSOPR, or a changer
move (`LibraryProvider` does the latter with a list of spare volumes for
*MOUNTED). Declining ends the operation (`Cancelled`).

The new volume must be labeled, writable, the one asked for, and hold no
active files. The file continues at its first file position as
`HDR1 HDR2 TM` with the next volume sequence number, the same file
sequence number and file set identifier, and the volume switch set.

Reading the file, an EOV trailer asks the provider the same way; the next
volume's first file must have the same file label and sequence number and
the next volume sequence number.

Unlabeled files do not span volumes; the end of the medium is an error.

## Tape: libraries and cartridges

`device::MediaLibrary` is a *TAPMLB device's cartridges and drives.

- **Cartridge identifiers** are the first six characters of the volume
  tag (bar code `E01001L5` is cartridge `E01001`).
- **sync** takes the changer's element status: new cartridges arrive in
  *INSERT (or *SYSGEN in a library without categories), locations and
  home slots follow the elements, and cartridges that left the library
  are dropped unless they are *EJECT.
- **Categories**: *INSERT, *EJECT, *SHARE400, *NOSHARE, *CNV, *IPL, *NL,
  *SYSGEN and user categories. ADDTAPCTG moves a cartridge from *INSERT
  to an assignable category (not *INSERT, *EJECT or *SYSGEN). CHGTAPCTG
  moves between assignable categories. RMVTAPCTG sets *EJECT and moves
  the cartridge to an empty import/export station (it stays in its slot
  when there is none). A *SYSGEN library refuses all three. A mounted
  cartridge cannot be removed.
- **VOL(volid)**: the cartridge must be in a usable category (not
  *INSERT or *EJECT). A drive already holding it is used as it is;
  otherwise the first drive not *DEALLOCATED (empty ones first, then
  *ALLOCATED before *UNPROTECTED) is chosen, its cartridge goes home and
  the volume moves in. The moves are returned for `execute` on the
  changer.
- **After use**: a cartridge goes back to its home slot; a *CNV cartridge
  goes to a station and becomes *EJECT.
- **Allocation**: *ALLOCATED (held by this system), *UNPROTECTED (used
  when needed), *DEALLOCATED (never used).

## Tape: resource names

Drives are the `stN` entries of `/sys/class/scsi_tape` (not `nstN` or
the mode variants `stNa`, `stNl`, `stNm`); changers are the `sg` nodes
of SCSI devices of type 8. `device::assign` names them from the scanned
list: devices sort by SCSI address (host, channel, target, LUN) and then
node number; a device whose serial number had a name keeps it, and the
rest take the lowest free numbers in that order, `TAP01`, `TAP02`, ...
for drives and `TAPMLB01`, ... for changers.

## Tape: message identifiers

| Error | Message |
|-------|---------|
| no volume loaded | CPF6760 |
| volume write protected | CPF6768 |
| end of volume, no further volume | CPF6718 |
| wrong volume mounted, or not the continuation | CPF6720 |
| labels damaged, unlabeled where labels are needed, or the reverse | CPF6772 |
| file not found by SEQNBR or FILELABEL | CPF6790 |
| active file would be overwritten | CPA4036 |
| changer failure | CPF6745 |
| device error, cancelled, bad parameter, record length | CPF6708 |
| cartridge not in the library | CPF6745 |
| category refuses the operation | CPF67D2 |
| cartridge in use | CPF6747 |
| no drive available | CPF6749 |

CPF6760 ("Device TAP01 not ready") is the oracle's own answer to
`DSPTAP` with no volume; the others are provisional until checked
against the oracle's message file (`docs/SVC_OPEN_QUESTIONS.md`,
Media).

## Tape: physical tape acceptance

The procedure Cody runs on hardware: a QLogic QLE2562 (8 Gb, ISP2532) in
the QS/400 machine, cabled point to point (or through one zone) to a
second QLE2562 in an mhVTL appliance that exports a virtual library over
Fibre Channel.

### The appliance

1. A Linux server (any current distribution with LIO, e.g. Ubuntu 24.04
   LTS) with the second QLE2562.
2. Install mhVTL from its source (`git clone` the mhvtl project, `make`,
   `make install`, build and load the `mhvtl` kernel module) and start
   `mhvtl.target`.
3. Configure `/etc/mhvtl/device.conf`: one library (an STK L700 or IBM
   TS3500 emulation) at `Library: 10`, two LTO-5 drives (`Drive: 11`,
   `Drive: 12`) with serial numbers, and `/etc/mhvtl/library_contents.10`
   with eight slots, one import/export element and cartridges
   `E01001L5` to `E01006L5`.
4. Create the media: `mktape -l 10 -m E01001L5 -t data -d LTO5 -s 20000`
   for each cartridge (20 GB), and two small ones for the multi-volume
   test: `mktape -l 10 -m E01009L5 -t data -d LTO5 -s 500` and
   `E01010L5` the same (500 MB each). Add them to the slots in
   `library_contents.10`. Restart mhVTL; `lsscsi -g` shows the changer
   and two drives with their `sg` nodes.
5. Put the appliance's HBA in target mode: kernel parameter
   `qla2xxx.qlini_mode=disabled`, reboot, check that `targetcli ls` lists
   its WWPN under `qla2xxx`.
6. In `targetcli`: create `pscsi` backstores for the changer's and both
   drives' SCSI devices (by their `H:C:T:L`), create the `qla2xxx` target
   on the HBA's WWPN, add an ACL for the QS/400 HBA's WWPN, map the
   changer as LUN 0 and the drives as LUNs 1 and 2, `saveconfig`.

### QS/400

1. The kernel carries `CONFIG_CHR_DEV_ST`, `CONFIG_CHR_DEV_SG`,
   `CONFIG_SCSI_FC_ATTRS`, `CONFIG_SCSI_QLA_FC`, `CONFIG_SCSI_LPFC` and
   `CONFIG_SCSI_SCAN_ASYNC=n` (`kernel/config/qs400.config`, checked by
   `cargo run -p build -- kernel`). The QLE2562 runs the firmware in its
   own flash; no firmware file ships in the image. If the driver logs
   that it cannot load firmware and the link stays down, note the HBA's
   flash firmware level: that is a finding, not a configuration step.
2. IPL. In the service console, `hw` and WRKHDWRSC TYPE(*STG) list the
   library as `TAPMLB01` with drives `TAP01` and `TAP02` (the drive with
   the lower LUN is TAP01).
3. Devices: with automatic configuration the description TAPMLB01 exists
   already; otherwise `CRTDEVMLB DEVD(TAPMLB01) DEVCLS(*TAP)
   RSRCNAME(TAPMLB01)`. For a standalone test later,
   `CRTDEVTAP DEVD(TAP01) RSRCNAME(TAP01)`.
4. `VRYCFG CFGOBJ(TAPMLB01) CFGTYPE(*DEV) STATUS(*ON)`.
5. `WRKMLBSTS`: TAPMLB01 varied on, TAP01 and TAP02 *ALLOCATED.
6. `WRKTAPCTG DEV(TAPMLB01)`: E01001 to E01006, E01009 and E01010 in
   *INSERT.
7. `ADDTAPCTG DEV(TAPMLB01) CTG(E01001 E01002 E01003 E01009 E01010)
   CGY(*SHARE400)`; WRKTAPCTG shows them in *SHARE400.
8. For each of them, `INZTAP DEV(TAPMLB01) NEWVOL(E01001) NEWOWNID(QS400)
   VOL(E01001) CHECK(*NO) DENSITY(*DEVTYPE) ENDOPT(*UNLOAD)` (with the
   matching identifier). `DSPTAP DEV(TAPMLB01) VOL(E01001)
   DATA(*LABELS)` shows the volume and no files.
9. `SAVLIB LIB(QGPL) DEV(TAPMLB01) VOL(E01001) ENDOPT(*REWIND)`.
10. `DSPTAP DEV(TAPMLB01) VOL(E01001) DATA(*SAVRST)` lists QGPL's
    objects; `DATA(*LABELS)` shows file 1 labeled QGPL.
11. `CRTLIB LIB(TAPETEST)`, then `RSTLIB SAVLIB(QGPL) DEV(TAPMLB01)
    VOL(E01001) RSTLIB(TAPETEST)`; the objects are in TAPETEST.
12. `CHKTAP DEV(TAPMLB01) VOL(E01001) SEQNBR(1) ENDOPT(*REWIND)` and
    `CHKTAP DEV(TAPMLB01) VOL(E01001) FILELABEL(QGPL)` complete;
    `CHKTAP ... SEQNBR(9)` fails with the file-not-found message.
13. `DUPTAP FROMDEV(TAPMLB01) TODEV(TAPMLB01) FROMVOL(E01001)
    TOVOL(E01002) FROMSEQNBR(*ALL) TOSEQNBR(*FROMSEQ)`; `DSPTAP` of
    E01002 shows the same files; `RSTLIB` from E01002 into another test
    library works.
14. Multi-volume: build a library larger than 500 MB (`CRTLIB TAPEBIG`,
    a physical file of 800000 records of 1000 bytes filled with
    `INSERT ... SELECT` from a recursive common table expression), then
    `SAVLIB LIB(TAPEBIG) DEV(TAPMLB01) VOL(E01009 E01010)
    ENDOPT(*UNLOAD)`. The save continues on E01010 without an inquiry
    (the volume is on the list). `DSPTAP ... VOL(E01009) DATA(*LABELS)`
    shows the file ending in EOV; on E01010 the file shows volume
    sequence 2. Repeat with `VOL(E01009)` alone: QSYSOPR gets the
    inquiry for the next volume; answer it after ADDTAPCTG/mount of
    E01010. `RSTLIB SAVLIB(TAPEBIG) DEV(TAPMLB01) VOL(E01009 E01010)
    RSTLIB(TAPEBIG2)` restores every record (compare the counts).
15. `ENDOPT(*UNLOAD)`: after step 14, WRKMLBSTS shows both drives empty
    and WRKTAPCTG shows E01009 and E01010 back in their slots.
16. `RMVTAPCTG DEV(TAPMLB01) CTG(E01003)`: E01003 goes to the
    import/export element and *EJECT; on the appliance
    `vtlcmd library show map` agrees.
17. Standalone drive: `VRYCFG CFGOBJ(TAPMLB01) CFGTYPE(*DEV)
    STATUS(*OFF)`, `VRYCFG CFGOBJ(TAP01) CFGTYPE(*DEV) STATUS(*ON)`, load
    E01004 on the appliance (`vtlcmd 11 load E01004L5`), `INZTAP DEV(TAP01)
    NEWVOL(E01004) CHECK(*NO)`, `SAVLIB LIB(QGPL) DEV(TAP01)`,
    `CHKTAP DEV(TAP01) VOL(E01004) ENDOPT(*UNLOAD)`; the appliance shows
    the drive unloaded.

Record for each step what the screen showed, and attach the appliance's
`/var/log/mhvtl` and the QS/400 job logs to the acceptance note.

## Optical

The crate `media/optical` (package `optical`) is everything QS/400 knows
about optical media: an ISO 9660 writer and reader with Joliet and Rock
Ridge, the hybrid install image, the install media manifest, writable
virtual volumes and the operations behind the optical commands, the
naming of physical drives, and the set of volumes `/QOPT` shows.

| Module | Does |
|---|---|
| `iso::write`, `iso::plan` | `IsoBuilder`: a tree of files, directories and links to an image, in one pass |
| `iso::read` | `IsoImage`: descriptors, a directory walk, names, extents, reads |
| `iso::name`, `iso::susp`, `iso::boot`, `iso::date` | level 2 and Joliet names, Rock Ridge entries, El Torito, timestamps |
| `iso::hybrid` | the protective MBR and GPT of the hybrid image, and their check |
| `manifest` | `QS400/MEDIA.MANIFEST`, the mark of installation media |
| `source` | `ReadAt`, `ImageStore`: images in memory, in files, in windows of devices |

### ISO 9660 as written

- Level 2 identifiers in the primary tree (d-characters, files `NAME.EXT;1`
  in 30 characters, directories in 31); a real name that does not fit is
  upper-cased, other characters become `_`, and clashes in a directory
  get `_1`, `_2` ... in name order.
- A Joliet tree (UCS-2 level 3, `%/E`) with the real names, 64 characters
  at most, none of `* / : ; ? \`.
- Rock Ridge (RRIP 1.10) in the primary tree: `SP` and `ER` on the root's
  `.`, `PX` (mode `r-x` for directories, `r--` for files, owner 0), `NM`
  with the real name and `TF` on every record. A record never needs a
  continuation area; a name too long for one record is refused.
- El Torito with one EFI no-emulation entry when a boot image is named.
- Files larger than an extent (4 GiB less one sector) are recorded as
  several extents with the multi-extent flag.
- A link is a second record pointing at another file's extents.
- Deterministic: one timestamp (the caller's, `SOURCE_DATE_EPOCH` for
  `build iso`) on every record and descriptor, records sorted by
  identifier, path tables in level, parent and identifier order, files in
  name order with the boot image last, GUIDs derived from the volume
  identifier and the timestamp.

The descriptors follow sector 16 in the order primary, El Torito boot
record (always sector 17 when present), Joliet, terminator; then the
boot catalog, the four path tables, the primary directories, the Joliet
directories and the files.

### ISO 9660 as read

The reader scans at most 64 descriptors from sector 16, needs a primary
descriptor with 2048-byte blocks, and walks directories rather than
trusting path tables. Names come from the Joliet tree when there is one,
else from Rock Ridge `NM` entries when the root's `.` carries `SP`, else
from the level 2 identifiers (without `;1` and a trailing dot). Rock
Ridge `CE` continuations are not followed. A directory is at most 16 MiB,
a walk at most 64 levels and a million entries, a directory met twice is
a loop; every extent must lie inside the image. Damage of any kind is
`Error::Malformed` (`OPT1605`), never a panic; a test mutates images
thousands of times to hold it to that. UDF is not read.

### Hybrid install image

`build iso` writes one image that works three ways: as a CD or virtual
optical volume (El Torito), written raw to a disk or stick (the GPT), and
with its tree copied onto a FAT32 stick, as Rufus does (the loose
`EFI/BOOT/BOOTX64.EFI`). The tree:

| Path | Holds |
|---|---|
| `EFI.IMG` | the El Torito boot image: a FAT16 volume with `EFI/BOOT/BOOTX64.EFI` |
| `EFI/BOOT/BOOTX64.EFI` | the media UKI (IPL type D) |
| `QS400/LIC/qs400.efi` | a second record for the same extents |
| `QS400/MEDIA.MANIFEST` | the manifest |
| `QS400A.EFI`, `QS400B.EFI` | the slot UKIs the installer copies onto the disk |
| `QS400.TXT` | a few lines about the media |

The image is laid out from one table of byte regions; the 2048-byte ISO
sectors and the 512-byte partition LBAs are both derived from it
(`iso::hybrid::HybridLayout`). The boot image is the last file, so the
two partitions do not overlap. `T` is the image size; the volume space in
the primary descriptor is all of it.

| Region | Offset | Length | ISO sector | LBA | Holds |
|---|---|---|---|---|---|
| protective MBR | 0 | 512 | 0 | 0 | one entry of type `0xEE` over the whole image |
| primary GPT header | 512 | 512 | 0 | 1 | revision 1.0, 128 entries of 128 bytes from LBA 2 |
| primary GPT entries | 1024 | 16384 | 0 | 2 | partitions 1 and 2, the rest zero |
| ISO 9660 payload | 32768 | to the boot image | 16 | 64 | descriptors, path tables, directories, files: partition 2 |
| EFI boot image | `EFI.IMG`'s extent | its length | its sector | sector × 4 | partition 1 |
| backup GPT entries | T − 16896 | 16384 | | T/512 − 33 | the same entries |
| backup GPT header | T − 512 | 512 | | T/512 − 1 | the header for the backup |

The tail after the boot image is 9 sectors (18432 bytes), enough for the
backup entries and header. Partition 1 has the EFI System type
`c12a7328-f81f-11d2-ba4b-00a0c93ec93b` and the name `QS400 EFI BOOT`;
partition 2 has the Quartz media type
`c8a1e5d2-7f34-4b6e-a2d9-515334304d44` and the name `QS400 MEDIA`. The
first usable LBA is 34, the last T/512 − 34. Disk and partition GUIDs are
derived from the volume identifier and the build time, so two builds
agree byte for byte.

`build iso` reads the result back with these readers before it writes
it: the El Torito entry is a bootable EFI no-emulation entry pointing at
`EFI.IMG`; `EFI.IMG` holds `EFI/BOOT/BOOTX64.EFI` and it is the media
UKI; the tree's `EFI/BOOT/BOOTX64.EFI` and `QS400/LIC/qs400.efi` are the
same bytes; partition 1 is exactly the boot image's LBAs, partition 2
runs from LBA 64 to just before it; the MBR, both headers and both entry
arrays check out (CRC-32 of header and array, backup at the last LBA,
backup equal to the primary).

The conformance harness boots it each way: `test --media iso` (the CD),
`--media iso-raw` (the image as a virtio disk) and `--media iso-fat` (the
tree copied onto an MBR disk with one FAT32 partition, built in Rust from
our own ISO reader). The last two also install onto a blank disk beside
the medium and boot it.

### Install media manifest

`QS400/MEDIA.MANIFEST` marks a file system as QS/400 installation media
and lists the payload. Text, LF line ends; the paths are level 2 names so
they read the same through every name form and FAT.

| Line | Fields |
|---|---|
| `QS400-MEDIA 1` | the first line, always |
| `release` | the release, `V1R0M0` |
| `volume` | the ISO volume identifier |
| `kind` | `install` (distribution media) or `savsys` (a `SAVSYS` copy of a system); absent means `install` |
| `file` | path, size in bytes, SHA-256 in lower-case hex; one line per payload file |
| `signature` | key ID (the first 8 bytes of the public key's SHA-256) and the ed25519 signature of every line before it, both hex; last |

The payload files are `QS400A.EFI`, `QS400B.EFI`,
`EFI/BOOT/BOOTX64.EFI` and the product loads `QS400/LP/<ID>.SAVF`
(`QS00SS1`, the operating system, and the licensed programs); see
docs/INSTALL.md for how the installer reads them and the response file
`QS400/INSTALL.RSP`.

On a D IPL, `slic` looks for the manifest on every block device in this
order: optical drives (`sr*`), removable disks, then the other disks,
each disk before its partitions. A device is mounted read-only only when
it looks like ISO 9660 (`CD001` at byte 32769) or FAT (`0x55AA` and a FAT
signature in the boot sector); the kernel mounts it as `iso9660` or
`vfat` at `/run/qs400/media`. The first device with a manifest is the
media; the slot UKIs are read from it and their sizes checked against
the manifest. The disk holding the media is never offered as a load
source. The kernel carries `ISO9660_FS`, `JOLIET`, `BLK_DEV_SR` and
`USB_STORAGE`, and not `ZISOFS`.

### Writable virtual volumes

IBM i makes a writable image with `ADDIMGCLGE FROMFILE(*NEW)` and
formats it as UDF. QS/400 keeps ISO 9660 (with Joliet and Rock Ridge,
as above) and adds a Quartz layout, so any ISO reader can still read a
volume QS/400 wrote. A volume is writable when its root holds the label;
every other ISO 9660 image is read only.

| Path | Kind | Holds |
|---|---|---|
| `/QS400VOL.LBL` | file | the volume label; hidden from listings, `/QOPT` and `DSPOPT` |
| `/<LIB>` | file | a save of library `LIB` with `OPTFILE(*)` (`SAVLIB`, `SAVOBJ`, `SAVCHGOBJ`) |
| `/QSAVIFS.nnn` | file | the `nnn`th `SAV` with `OPTFILE(*)` |
| `/QSAVSYS/<KIND>` | file | a system save (`SAVSYS`, `SAVSECDTA`, `SAVCFG`) |
| anything else | either | files and directories written by name (`OPTFILE('/path')`, `/QOPT`) |

The save names follow IBM's rule as far as it is documented: with
`OPTFILE(*)` a save goes to a file in the volume's root named after the
library. The `SAV` and system names are Quartz's choice. The bytes of a
save file are the save stream; this layout does not look inside them.

The label is text, LF line ends: the line `QS400-OPTVOL 1`, then one
`key<TAB>value` line per key in this order. Values escape `\` as `\\`,
tab as `\t`, newline as `\n` and carriage return as `\r`. Unknown keys
are ignored.

| Key | Value |
|---|---|
| `volid` | the volume identifier |
| `text` | the text description, 50 characters at most |
| `capacity` | the image may grow to this many bytes (`IMGSIZ`) |
| `created` | Unix time of `INZOPT` or the `*NEW` image |
| `changed` | Unix time of the last write; every timestamp in the image is this one |

A volume identifier is 1 to 32 characters of `A-Z 0-9 $ # @ _ . -`,
upper-cased, starting with a letter, a digit, `$`, `#` or `@` (Quartz's
reading of IBM's rules). The primary descriptor carries it as
d-characters (others become `_`), Joliet the first 16 characters; the
label carries it whole and wins. File and directory names are Joliet's:
64 characters, none of `* / : ; ? \`; they match without regard to case.

Writing is a session (`optical::Volume`): unchanged files stay extents
of the image, changed files are held in memory, and `flush` rewrites the
whole image through `ImageStore::replace` (a file image is written beside
the old one and renamed over it). The same tree and the same `changed`
time give the same bytes. Each change is checked against the capacity
first and refused whole (`OPT1335`) when the rewritten image would not
fit; a write-protected volume refuses every change (`OPT1520`).

| Command | Operation |
|---|---|
| `INZOPT NEWVOL CHECK CLEAR TEXT` | `volume::ops::inzopt`: `CHECK(*YES)` refuses a volume holding files (`OPT1330`); `CLEAR(*YES)` overwrites the old image with zeros first; capacity kept from the old label |
| `DSPOPT DATA(*VOLATR)` | `ops::volume_attrs`: identifier, text, `*ISO9660`, capacity, used, free, block size 2048, dates, set, protection |
| `DSPOPT DATA(*DIRATR)` | `ops::dir_attrs`: each directory with its file, directory and byte counts |
| `DSPOPT DATA(*FILATR)` | `ops::file_attrs`: each file with its size |
| `CHKOPT` | `ops::chkopt`: reads every file to its end; problems listed with their message |
| `DUPOPT FROMVOL TOVOL NEWVOL` | `ops::dupopt`: every file and directory into another image, as a writable volume |

### Devices and /QOPT

`device::scan` lists `sr*` drives and removable USB disks from a
`/sys/class/block` style directory; `device::name_resources` names them
`OPT01`, `OPT02` ... drives first, then USB disks, each in natural
kernel-name order (`sr2` before `sr10`, `sdb` before `sdaa`), so the
same devices always get the same names. `device::open_media` finds the
ISO 9660 volume on a device: at its start (a disc, the hybrid image
written raw), else at the start of a GPT or MBR partition. A FAT copy of
the media is not an optical volume; the D-IPL locator above reads it.

`mount::VolumeSet` holds the loaded volumes by identifier. The IFS shows
it as `/QOPT/<volume>/...` through `ifs::vfs::optical::Volumes`: ISO
images read only (`EROFS`), writable volumes read-write. A file opened
for writing is held whole and put back at close or `fsync`, which also
flushes the volume; `mkdir`, `unlink` and `rmdir` flush at once. Names
fold. A writable volume grants `*PUBLIC *ALL`; a read-only one `*RX`.

| Error | Message | `errno` |
|---|---|---|
| damaged image, I/O | `OPT1605` | `EIO` |
| not found | `OPT1115` | `ENOENT` |
| already there | `OPT1210` | `EEXIST` |
| not a directory, a directory, not empty, bad request | `OPT1203` | `ENOTDIR`, `EISDIR`, `ENOTEMPTY`, `EINVAL` |
| name not allowed | `OPT1317` | `ENAMETOOLONG` |
| volume identifier not allowed | `OPT1331` | `EINVAL` |
| volume full | `OPT1335` | `ENOSPC` |
| write protected, read only | `OPT1520` | `EROFS` |
| `INZOPT CHECK(*YES)` found files | `OPT1330` | `EINVAL` |

The message identifiers are Quartz's choice in IBM's `OPT` range; IBM's
texts for them are not reproduced.

## Image catalogs

The crate `media/imgclg` (package `imgclg`) is the `*IMGCLG` object:
catalogs of virtual optical images (`TYPE(*OPT)`) and virtual tape
volumes (`TYPE(*TAP)`) kept in a directory, over the small file-system
trait `CatalogFs` (`StdFs` over a host directory, `MemFs` in memory, the
IFS later). Tape volume files belong to the tape crate: a `*TAP` catalog
makes and reads them through the `TapeImages` trait.

| Module | Does |
|---|---|
| `model` | `Catalog`, `Entry`, the statuses, the metadata file |
| `ops` | `Registry`: every command, the panels' data, what `/QOPT` presents |
| `fs` | `CatalogFs`, `StdFs`, `MemFs` |
| `tape` | `TapeImages`, `NoTape` |

### States

A catalog is `*READY` when `LODIMGCLG OPTION(*LOAD)` has loaded it on a
virtual device, else `*NOTREADY`. One device holds one catalog, and a
loaded catalog is on one device. An entry is:

| Status | Meaning |
|---|---|
| `*MOUNTED` | loaded, and the volume the device presents; one per catalog at most |
| `*LOADED` | in the device, ready to be mounted |
| `*AVAILABLE` | in the catalog only; `LODIMGCLGE OPTION(*LOAD)` makes it usable |

A new entry is `*LOADED`. Whenever a `*READY` catalog has no mounted
entry (it was just loaded, the mounted entry was unloaded or removed),
the lowest-index `*LOADED` entry is mounted. `LODIMGCLGE OPTION(*MOUNT)`
mounts an entry and returns the old one to `*LOADED`; `*UNLOAD` makes it
`*AVAILABLE`. Statuses are kept while the catalog is not ready. `/QOPT`
shows the mounted entry of each `*READY` `*OPT` catalog
(`Registry::sync_volumes` brings a `VolumeSet` in line, leaving volumes
it did not add alone).

### Commands

| Command | Parameters | Does |
|---|---|---|
| `CRTIMGCLG` | `IMGCLG DIR TYPE CRTDIR ADDVRTVOL TEXT` | a catalog over a directory no other catalog uses; `ADDVRTVOL(*DIR)` adds the images already there (in `VOLUME_LIST` order when that file exists, one name a line, `#` comments; else by name), `ADDVRTVOL(n)` makes `n` new volumes of `IMGSIZ` |
| `DLTIMGCLG` | `IMGCLG KEEP` | not while `*READY`; `KEEP(*NO)` deletes the images and the directory |
| `ADDIMGCLGE` | `IMGCLG FROMFILE TOFILE IMGCLGIDX IMGSIZ VOLNAM WRTPTC TEXT` | `FROMFILE(name)` takes a file in the directory, `FROMFILE('/path')` copies it in (`TOFILE(*FROMFILE)`, a name, or `*GEN`: `<catalog><nnn>`, `.ISO` for optical); `FROMFILE(*NEW)` makes an empty writable volume of `IMGSIZ` MiB (optical) or calls the tape constructor; `IMGCLGIDX(*AVAIL)` takes the lowest free index; volumes are unique in a catalog; a read-only image is always write protected |
| `RMVIMGCLGE` | `IMGCLG IMGCLGIDX KEEP` | `KEEP(*NO)` deletes the image |
| `CHGIMGCLGE` | `IMGCLG IMGCLGIDX WRTPTC TEXT` | `WRTPTC(*NO)` only for a writable image |
| `LODIMGCLGE` | `IMGCLG IMGCLGIDX OPTION` | `*LOAD`, `*UNLOAD`, `*MOUNT` as above |
| `LODIMGCLG` | `IMGCLG DEV OPTION` | `*LOAD` on a device, `*UNLOAD` |
| `VFYIMGCLG` | `IMGCLG TYPE SORT` | every image is there, reads and still carries its volume; for `TYPE(*PTF)`, `*LIC` and `*UPGRADE` the optical volumes also form one set (one volume set identifier and size), complete (as many entries as the set size), each sequence number once and in order; for `*LIC` and `*UPGRADE` volume 1 holds the install manifest; `SORT(*YES)` renumbers the entries in sequence order instead of reporting them out of order |
| `WRKIMGCLG` | | `Registry::wrkimgclg`: name, status, type, text |
| `WRKIMGCLGE` | `IMGCLG` | `Registry::wrkimgclge`: the catalog's status, type, device, directory, and per entry index, status, write protection, volume, file, text |

`IMGSIZ` is a number of MiB or `*CD650`, `*DVD2600`, `*DVD4700`. A
catalog holds 256 entries.

### Catalog metadata file

Each catalog keeps `QS400CLG.INF` in its directory, rewritten whole on
every change. Text, LF line ends: the line `QS400-IMGCLG 1`, then the
catalog's `key<TAB>value` lines in this order, then one `entry` line per
entry in index order. Values escape as the volume label's do.

| Key | Value |
|---|---|
| `name` | the catalog name |
| `type` | `*OPT` or `*TAP` |
| `dir` | the directory, absolute |
| `text` | the text description |
| `status` | `*READY` or `*NOTREADY` |
| `device` | the device it is loaded on, empty when not |
| `created` | Unix time of `CRTIMGCLG` |

An `entry` line is `entry` and these fields, tab separated:

| Field | Value |
|---|---|
| `index` | 1 to 256 |
| `file` | the image file's name in the directory |
| `volume` | the volume identifier |
| `status` | `*MOUNTED`, `*LOADED` or `*AVAILABLE` |
| `wrtptc` | `*YES` or `*NO` |
| `text` | the entry's text |

`Registry::load` rebuilds the catalogs from these files.

| Error | Message |
|---|---|
| catalog not found | `CPFBC01` |
| catalog already exists | `CPFBC02` |
| directory belongs to another catalog | `CPFBC03` |
| directory missing (`CRTDIR(*NO)`) | `CPFBC04` |
| no entry at the index | `CPFBC05` |
| index in use | `CPFBC06` |
| catalog full | `CPFBC07` |
| image file missing | `CPFBC08` |
| not an image of the catalog's type | `CPFBC09` |
| catalog loaded | `CPFBC0A` |
| device in use | `CPFBC0B` |
| volume already in the catalog | `CPFBC0C` |
| verification failed | `CPFBC0D` |
| image can only be read | `CPFBC0E` |
| parameter not allowed | `CPFBC0F` |
| metadata file damaged, file system | `CPFBC10` |

The `CPFBC` identifiers are Quartz's choice; optical errors keep their
`OPT` identifiers.

## In the storage service

`lic/slic/src/stgsvc/media/` puts the three crates to work; the command
processors are `lic/slic/src/cpp/medcmd.rs` (devices, catalogs, tape,
optical and cartridge commands, the panels' data), `tapcpy.rs` (tape
files, `CPYTOTAP`, `CPYFRMTAP`) and the save and restore commands of
`savcmd.rs`, `lpcmd.rs` and `ptfcmd.rs`. The `medio` crate holds the
parts that need no service: save streams on tape and optical
(`tapefile`, `optfile`), the catalog's next volume (`catalog`), the
virtual library (`library`) and tape files (`tapf`).

| Module | Does |
|---|---|
| `devices` | the `*DEVD` objects of class `*TAP`, `*OPT` and `*TAPMLB` (the `MediaDevice` section, `docs/WORKMGMT.md`), vary on and off, `QAUTOCFG`, `DSPDEVD` |
| `catalogs` | the `*IMGCLG` objects in `QUSRSYS` and the `imgclg::Registry` over the IFS root; `VirtTapes`, the `TapeImages` of virtual tape volumes |
| `drive` | a device opened for one operation: virtual tape over its catalog, a virtual library's drive, `/dev/nstN` for a real drive, the mounted optical volume |
| `library` | virtual media libraries: the catalog's entries as cartridges, kept in `QS400MLB.INF` |
| `stream` | the media parameters of the save and restore commands, and the one call that writes or reads a save's stream (across optical volumes too) |
| `operator` | inquiries (with the replies of "Operator replies") and attention messages to `QSYSOPR`, product activity log entries and problems for device and media errors |

### Devices

`CRTDEVTAP`, `CRTDEVOPT` and `CRTDEVMLB` make a device description in
`QSYS`. `RSRCNAME(*VRT)` makes a virtual device: its resource is the
next free `TAPVRTnn` or `OPTVRTnn`, its type and model IBM's for
virtual devices (`63B0 001` tape, `632B 001` optical). A real resource
is a name of the hardware table: `TAPnn` and `TAPMLBnn` from
`tape::device::naming` over `/sys/class/scsi_tape` and the type-8
`scsi_generic` nodes, `OPTnn` from the block devices; a real device's
type and model are Quartz's generic ones (`3580 004` tape, `632C 002`
optical, `3573 001` library) since the drive's own are not read.
`CHGDEVxxx` changes what was given; a new `RSRCNAME` needs the device
varied off.

`VRYCFG CFGTYPE(*DEV)` of a media device sets its status:

| Status | When |
|---|---|
| `VARIED OFF` | varied off, and after IPL for `ONLINE(*NO)` |
| `VARIED ON` | a tape or library device varied on |
| `ACTIVE` | an optical device varied on |
| `UNAVAILABLE` | varied on while another varied-on device has the resource |
| `FAILED` | varied on and the resource is not in the hardware table (`CPD26D4`) |

At service start every media device described `ONLINE(*YES)` is varied
on, and with `QAUTOCFG` on a description named as the resource is made
for each tape drive, changer and `sr` optical drive no description
names (text "Created by automatic configuration"). A device is deleted
only varied off and with no catalog loaded on it. Commands that use a
device need it varied on (`CPF6721`).

### Image catalogs over the IFS

An `*IMGCLG` object in `QUSRSYS` holds the line `QS400-IMGCLG-OBJ 1`
and the catalog's directory; the catalog itself is the metadata file
there. At service start the registry is rebuilt from the objects whose
directories still hold their metadata file. The catalogs reach the IFS
root's xfs directly (`imgclg::StdFs` at `/run/qs400/ifs`), as the
service that owns it; their files carry no IFS metadata of their own.
`ADDIMGCLGE FROMFILE(*NEW)` in a `*TAP` catalog makes a virtual tape
volume of `IMGSIZ` MiB and labels it with its volume name. A catalog
loads (`LODIMGCLG`) on a virtual device of its type, varied on or not.
After every catalog command the volume set `/QOPT` shows is brought in
line (`Registry::sync_volumes`); the IFS is built with
`Ifs::with_optical` over that set.

### Save and restore on media

Every save builds its stream in memory as before; `stream::save`
then writes it to the device `DEV` names, and `stream::restore` reads it
back before the restore starts (the `medio` crate does the writing and
reading: `MediaWriter`, `MediaReader`).

- **Tape**: a standard-labeled file per library, record format U in
  64 KiB blocks: a `SAVLIB` (`SAVOBJ`, `SAVCHGOBJ`) of several libraries
  with `LABEL(*LIB)` writes one save stream per library, each labeled
  with its library's name, at `SEQNBR` and the sequence numbers after it
  (the files before the last end with `ENDOPT(*LEAVE)`, the last with
  the command's; each starts on the volume the one before ended on).
  `RSTLIB` of several libraries reads each library's file by its label.
  `QFILEIOC` labels `SAVCFG`, `QFILEUPR` `SAVSECDTA`, `QSAVIFS` `SAV`,
  the product identifier `SAVLICPGM`, the PTF's name `LODPTF`.
  `SEQNBR(*END)` appends; `EXPDATE` defaults to `*PERM`; `CLEAR` other
  than `*NONE` lets the save overwrite files that have not expired,
  else the inquiry `CPA4036` is asked: `I` overwrites, anything else
  ends the save (`CPF3794`). `SEQNBR(*SEARCH)` with `LABEL(*SAVLIB)`
  finds the first file with the library's label. A save to a save file
  stays one stream.
- **Multi-volume**: on a virtual device the next volume is the `VOL`
  list's next one, or for `VOL(*MOUNTED)` the catalog's next loaded
  entry after the mounted one (`medio::CatalogTapes`); the last volume
  used stays mounted. When there is none the inquiry `CPA4075` is
  asked (see "Operator replies"): `G` looks again over every loaded
  entry not used yet in the operation, anything else cancels. On a real
  drive `G` means the operator has put the next volume in.
- **Optical**: the file `OPTFILE` names on the mounted volume of the
  device's catalog (`'*'` the generated name of the Optical section);
  an existing file is replaced only with `CLEAR`, else the inquiry
  `OPT1260` is asked: `G` replaces it. A save larger than the volume's
  room continues on the next volume, chosen as for tape (the `VOL`
  list, else the catalog's next loaded entry, else the operator); see
  "Optical saves across volumes".
- **ENDOPT(*UNLOAD)** on a virtual tape device returns the entry in use
  to `*LOADED` and mounts the next loaded entry after it (or the first
  one before it); with no other loaded entry it stays mounted.
  `*REWIND` and `*LEAVE` keep it mounted.
- `SAV` and `RST` take `DEV('/QSYS.LIB/TAP01.DEVD')` or the name alone.
- `DSPTAP DATA(*SAVRST)` reads every file in the `SEQNBR` range (and
  `LABEL`) and lists it as `DSPSAVF` does (`savcmd::stream_listing`);
  `DSPOPT DATA(*SAVRST)` does the same for the files in the volume's
  root and `/QSAVSYS`.

### Tape and optical commands

| Command | Does |
|---|---|
| `INZTAP` | `tape::volume::inztap`; `NEWVOL(*CTGID)` takes `VOL` |
| `DSPTAP` | `DATA(*LABELS)` every file's labels, `*SAVRST` the saves, `*VOLLBL` the volume label |
| `CHKTAP` | `tape::volume::chktap`; `SEQNBR(*END)`, `*ONLY`, a number |
| `DUPTAP` | `tape::volume::duptap` between two virtual tape devices |
| `INZOPT` | `optical::volume::ops::inzopt` on the entry's image; the entry is put back in the catalog under its new identifier |
| `DSPOPT` | `*VOLATR`, `*DIRATR`, `*FILATR`, `*SAVRST` |
| `CHKOPT` | `ops::chkopt` (a QS/400 command; IBM i has none) |
| `DUPOPT` | `ops::dupopt` onto another catalog entry's image |
| `WRKOPTVOL` | the mounted volumes of the optical devices |
| `WRKMLBSTS` | the library devices; for a virtual one its drives, their allocation and cartridges |
| `WRKTAPCTG`, `DSPTAPCTG` | a library's cartridges: identifier, label volume, category, status (`Inserted`, `Available`, `Mounted`, `Ejected`), place, home slot |
| `ADDTAPCTG` | `CTG` (or `*INSERT` for all of them) from `*INSERT` into `CGY`; `CHECK(*YES)` refuses a cartridge whose label names another volume (`CPF6720`) |
| `CHGTAPCTG` | between assignable categories (`*SAME` changes nothing) |
| `RMVTAPCTG` | to `*EJECT` and an empty import/export station (it stays in its slot when there is none) |
| `CPYTOTAP`, `CPYFRMTAP` | see "Tape files and copies" |

### Virtual media libraries

`CRTDEVMLB RSRCNAME(*VRT)` makes a virtual library: its resource is a
`TAPVRTnn` name, and the `*TAP` image catalog `LODIMGCLG` loads on it
holds its cartridges (`stgsvc::media::library`, over
`medio::library::CatalogLibrary` and the tape crate's
`VirtualLibrary` and `MediaLibrary`). Every entry not `*AVAILABLE` is a
cartridge; its identifier is its volume tag (the entry's volume name).
The library has 256 storage slots (the entry at index `n` arrives in
slot `n`), 4 import/export stations and 2 drives, `<rsrc>D1` and
`<rsrc>D2`, all `*ALLOCATED` at first.

- **Insert**: an entry added or loaded into the catalog arrives in
  `*INSERT`; `ADDTAPCTG` makes it usable.
- **Mount**: a command on the library with `VOL(x)` checks every
  volume of the list is in a usable category (`CPF67D2`), then
  `MediaLibrary::select` picks the drive and the changer moves the
  cartridge in (the drive's cartridge goes home first). `VOL(*MOUNTED)`
  uses the cartridge in the first drive holding one (`CPF6760` when
  none does). Further volumes of a multi-volume file are moved in
  (`medio::LibraryVolumes`): a volume the list names, or for
  `*MOUNTED` the next spare cartridge in a usable category once the
  operator answers `CPA4075` with `G`.
- **Demount**: a cartridge stays in its drive after the command unless
  the command unloaded it (`ENDOPT(*UNLOAD)`); then it goes home (a
  `*CNV` cartridge goes to a station as `*EJECT`). `WRKTAPCTG` option 8
  demounts by hand, option 6 mounts.
- **Eject**: `RMVTAPCTG`. A cartridge in a station leaves the library
  when its catalog entry is unloaded (`LODIMGCLGE OPTION(*UNLOAD)`);
  loaded again it arrives in `*INSERT`.
- **Allocation**: `WRKMLBSTS` options 4, 5 and 6 make a drive
  `*ALLOCATED`, `*UNPROTECTED` or `*DEALLOCATED`; a deallocated drive
  is never chosen.

Between commands the library is kept in `QS400MLB.INF` in the catalog's
directory (the library is built again from the entries and this file
for every command). Text, LF line ends: the line `QS400-MLB 1`, then one
line per drive and one per cartridge, fields tab separated:

| Line | Fields |
|---|---|
| `drive` | number from 1, allocation (`*ALLOCATED`, `*UNPROTECTED`, `*DEALLOCATED`) |
| `ctg` | cartridge identifier, category, place (`slot:A`, `drive:A`, `station:A` with the element address, or `outside`), home slot's element address or `-` |

Unknown lines are skipped. A real library's changer (`SgChanger`) is
not driven by the commands in this release: they refuse with `CPF6745`
on a library with a real resource.

### Optical saves across volumes

When a save does not fit on the volume, `medio::optfile::OpticalWriter`
writes as much as fits (whole 2048-byte sectors, 32 KiB kept free for
the directory records) as part 1 under the save's path, with the part's
label beside it at `<path>.QSPART`, asks its `OpticalVolumes` for the
next volume (`volume_seq` the part wanted), and goes on with part 2
under the same path there, until the last part. A save on one volume
has no label, so its file stays the bare stream. The label is text, LF
line ends:

| Line | Value |
|---|---|
| `QS400-OPTPART 1` | the first line |
| `save` | the save's identifier, the same on every part (FNV-1a 64 of the stream and its length, hex) |
| `part` | the part's number, from 1 |
| `more` | `1` when another part follows, else `0` |
| `bytes` | bytes in this part |

Reading starts on the volume holding part 1 (another part is refused,
`OPT1115`) and follows `more` through the same next-volume choice; each
next part must carry the same save identifier and the next number.

### Tape files and copies

A tape file is a `*FILE` with attribute `TAPF` (`CRTTAPF`, `CHGTAPF`,
`DLTF`, `DSPFD`; `stgsvc` and `cpp/tapcpy.rs`). Its payload is
`medio::tapf::TapeFileDef` as text: the line `QS400-TAPF 1`, then
`key<TAB>value` lines `dev` (device names, blank separated), `vol`,
`reels` (`*SL`, `*NL`), `seqnbr` (a number or `*END`), `label`, `rcdlen`
and `blklen` (0 for `*CALC`), `rcdblktyp` (`*F`, `*FB`, `*V`, `*VB`,
`*U`), `code` (`*EBCDIC`, `*ASCII`), `expdate` (`*NONE`, `*PERM` or
`year.day`), `endopt`, `text`. Unknown keys are skipped.

`CPYTOTAP` writes a member's records (a source member's data part) as
one tape file; `CPYFRMTAP` reads one back into a member (added when it
is missing; `MBROPT(*NONE)` refuses a member with records, `*ADD`
appends, `*REPLACE` clears) or lists it with `TOFILE(*PRINT)`
(`OUTFMT(*HEX)` adds the hexadecimal). Every parameter whose value is
`*TAPF` takes the tape file's. The records become blocks by the record
format (`tape::volume::records_to_blocks`, "HDR2" above); `*CALC`
lengths are the longest record (plus 4 for `V`) and, for blocked
records, the largest multiple of the record length up to 32760 bytes
(32760 for `VB`). `CODE(*ASCII)` writes and reads the data in ASCII.
`TOLABEL(*FROMMBR)` labels the file with the member's name. An active
file in the way asks `CPA4036` as a save does. `OVRTAPF` is not
provided.

### Operator replies

Media inquiries (`CPA4075` next volume, `CPA4036` active file,
`OPT1260` optical file exists) go to `QSYSOPR` from program `QTAERR`.
The reply comes from, in order:

1. a reply the operator already sent (`SNDRPYMSG`, `WRKMSG`) to the
   same inquiry (same identifier and data) that is still waiting or was
   answered and not yet taken: the reply is taken (the reply message is
   marked old);
2. the job's `INQMSGRPY`: `*DFT` takes the default; `*SYSRPYL` the
   system reply list's entry (`ADDRPYLE`; `*DFT` there is the default);
   the inquiry is then shown answered with that reply;
3. otherwise (`*RQD`, or no reply list entry) the inquiry is left
   waiting on `QSYSOPR` and the operation ends as for cancel. Running
   it again after the operator's reply takes that reply (step 1).

The service answers its requests one at a time, so it cannot wait for
a reply in the middle of a command; the third case is how a job that
needs the operator gets the reply.

### Displays

At a display `WRKIMGCLG` (F11 switches status and texts; options 1, 4,
8, 9, 10 run their commands, 12 shows the entries), `WRKIMGCLGE`
(options 2, 4, 6, 8, 9; F6 adds an entry), `WRKTAPCTG` (options 1, 2,
4, 5; 6 and 8 mount and demount) and `WRKMLBSTS` (options 1 and 2 vary
the library, 4 to 6 set a drive's allocation, 8 its cartridges) are
the panels of `QSMEDIA` (`work/qsjob/src/panels/work2/media.rs`; data
through `panel2_data`, moves and allocation through `service_op`
`mlb-*`). Without a display the service answers with the text forms.
`WRKIMGCLG` follows pub400's capture row for row; the others' geometry
is Quartz's (no capture).

### Errors

| Event | Operator | Logged |
|---|---|---|
| wrong volume, not ready, write protected, end of volume, unlabeled where labels are needed | attention message (`CPF6720`, `CPF6760`, `CPF6768`, `CPF6718`, `CPF6772`, `OPT1520`, `OPT1335`) | |
| drive or changer failure | attention message (`CPF6708`, `CPF6745`) | product activity log `A6006300` with the resource, a problem |
| damaged labels, frames or image | attention message (`CPF6772`, `OPT1605`) | product activity log `A6006310` with the resource, a problem |
| next volume needed | inquiry `CPA4075` (C cancel, G look again), default C | |
| active file in the way | inquiry `CPA4036` (C cancel, I overwrite), default C | |
| optical file exists | inquiry `OPT1260` (C cancel, G replace), default C | |

A product activity log entry of a device carries its resource name (ten
bytes after the entry's 17; the Phase 1 entries have none) and shows it
in DST's log.
