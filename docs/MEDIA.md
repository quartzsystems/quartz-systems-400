<<<<<<< HEAD
# Media: tape, optical, image catalogs

Save and restore write to three classes of media: tape (real drives and
media libraries, and virtual volumes in image catalogs), optical, and the
image catalogs that hold virtual volumes as stream files. This document
specifies the on-disk formats and the models; the commands that use them
(INZTAP, SAVLIB, WRKTAPCTG, ...) are wired in by `slic` and the command
processing programs.

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

The identifiers other than CPF6760 and CPF6720 are provisional until
checked against the oracle's message file.

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

(Written with the optical crate.)

## Image catalogs

(Written with the image catalog support.)
=======
# QS/400 removable media: tape, optical and image catalogs

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
| `file` | path, size in bytes, SHA-256 in lower-case hex; one line per payload file |

The payload files are `QS400A.EFI`, `QS400B.EFI` and
`EFI/BOOT/BOOTX64.EFI`.

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

## Image catalogs
>>>>>>> 8490bda7 (docs: MEDIA.md hybrid install image, checked by a test)
