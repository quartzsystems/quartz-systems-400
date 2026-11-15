# QSI: the host families (draft for `docs/QSI.md`)

The families a QS/VS virtual server host answers beside base and pairing,
version 1 of each, as `lic/qsi` has them (`src/host/`). This is a draft
the lead merges into `docs/QSI.md`; the session (`lic/qsisess`) and the
trust rules (`lic/qsitrust`) are documented there.

Every body is an `ipc::Wire` value read leniently: fields a later
version adds go at the end and are passed over. A one-byte enumeration
marked *open* keeps a value it does not know as `Other(n)`. Field names
and units are QS/VS's (`part/model`, `part/resource`, `part/partd`,
`stg/poold`, `net/vswitchd`); every identifier a console shows (a state's
words, a reference code and its text, a message) is passed as the host
gives it. Times are seconds since the epoch (`time`, `created`) as QS/VS
keeps them, except a reference code's `time_ms`, which is the base
family's `Refcode`. Processing units are hundredths of a core (QS/VS's
`Units`); memory is MiB; sizes in bytes unless the name says `mib`.

A reply has its request's operation number. A request that opens a
stream is answered by a reply whose body describes the stream (its size,
its name); the session gives the stream's number. Every family's
`Request` has `check()`, the protocol's bounds (`AnyRequest::check`
dispatches), refused with `QSI0002` before the host acts:

| Bound | Limit |
|---|---|
| a partition ID | 1 to 999 (0 only where a request says "every partition") |
| a name a console sends | 64 bytes |
| a text a console sends (a backing, a location code) | 256 bytes |
| a list a console sends | 256 items |
| a shut down's delay, base's power down's delay | 99999 seconds |
| a panel function | 1 to 99 |
| serial bytes in one piece of a console stream | 16 KiB (a piece is one frame) |

The host's own rules (a name's form, memory in blocks) are the host's to
refuse with its own messages.

## Capability bits and versions

| Family | Number | Version | Capability (`caps::`) | Bit |
|---|---|---|---|---|
| base | `0x0001` | 1 | `BASE` | 0 |
| pair | `0x0002` | 1 | `PAIR` | 1 |
| host | `0x0010` | 1 | `HOST` | 8 |
| partitions | `0x0011` | 1 | `PARTITIONS` | 9 |
| partition | `0x0012` | 1 | `PARTITION` | 10 |
| profiles | `0x0013` | 1 | `PROFILES` | 11 |
| panel | `0x0014` | 1 | `PANEL` | 12 |
| refcodes | `0x0015` | 1 | `REFCODES` | 13 |
| dumps | `0x0016` | 1 | `DUMPS` | 14 |
| console | `0x0017` | 1 | `CONSOLE` | 15 |
| vres | `0x0018` | 1 | `VRES` | 16 |

A QS/VS host says `caps::HOST_V1`, every bit above. A later version of
a family has a bit of its own.

## Shared types (`host::model`)

| Type | Fields |
|---|---|
| `PartitionId` | `u32`, 1 to 999 |
| `PartitionState` (open) | `NotActivated` 1, `Starting` 2, `Running` 3, `ShuttingDown` 4, `Error` 5: the names QS/MC shows. A partition held for its pool's space is Running with `A2005160` as its code (QS/VS's open question 34) |
| `Environment` (open) | `Standalone` 1: the operating system a standalone system runs (QS/VS's one environment; the crate names no product) |
| `IplSource` (open) | `A` 1, `B` 2, `Recorded` 3, `D` 4 |
| `IplType` (open) | `Unattended` 1, `Attended` 2 |
| `ConsoleMode` (open) | `Serial` 1, `Local` 2, `Partition` 3 |
| `ProcessorMode` (open) | `Dedicated` 1, `Shared` 2 |
| `Sharing` (open) | `Capped` 1, `Uncapped` 2 |
| `base::Keylock` (open) | `Normal` 1, `Manual` 2, and, added for QS/VS, `Auto` 3, `Secure` 4 |
| `Note` | `msgid`, `text`, `help`: a message the host issued, where an event or a list carries one |
| `AutoStart` | `enabled`, `order`, `delay_secs`, `wait`, `wait_secs` |
| `Definition` | `id`, `name`, `environment`, `serial`, `keylock`, `default_profile`, `auto_start`, `created` (s) |
| `Processors` | `mode`, `minimum`, `desired`, `maximum`; shared: `minimum_units`, `desired_units`, `maximum_units` (hundredths), `sharing`, `weight` (0 to 255), `pool` |
| `Memory` | `minimum_mib`, `desired_mib`, `maximum_mib` |
| `Lun`, `ScsiAdapter` | `lun`, `backing` (`POOL/NAME`), `serial`; `slot`, `luns` |
| `Boot` | `source`, `ipl_type`, `console` |
| `EthernetAdapter` | `slot`, `switch`, `vlan`, `tagged`, `mac`, `required` |
| `MediaDevice` | `slot`, `media` (an image or volume name) |
| `PhysicalIo` | `location`, `required` (carried so a profile reads whole; managed in a later version) |
| `Profile` | `name`, `processors`, `memory`, `boot`, `scsi`, `ethernet`, `optical`, `tape`, `physical_io` |
| `Current` | `mode`, `cores` (dedicated), `pool`, `units` (hundredths), `virtuals`, `capped`, `weight`, `memory_mib`: QS/VS's `resource::Current` flattened, so a later mode is an open value, not an unknown tag |
| `Channel` | `up`, `none`, `responding`, `kept`, `system`, `release`, `fix_level`, `addresses` (`address/prefix`), `version`, `caps` (the partition scope's, `qsipart::caps`) |
| `Holder` | `partition` (0 for none), `slot` |

## host (`0x0010`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `GetHost` (empty) | `Host`: `sockets`, `cores`, `threads_per_core`, `processors` and `memory` as `Amount` {`installed`, `configurable`, `available`} (hundredths of a core; MiB), `memory_block_mib`, `reserved_cores` (by number), `reserved_memory_mib` (the host reserve), `pools`: `ProcPool` {`name`, `cores`, `maximum`, `entitled`, `available` (hundredths), `partitions`}, the default first; `partitions` defined, `max_partitions` |
| `0x0002` | `GetSettings` (empty) | `Settings`: `max_partitions`, `memory_block_mib`, `reserved_cores`, `reserved_memory_mib`, `mac_prefix`, and `[service]`'s `delayed_grace_secs`, `system_timeout_secs`, `host_timeout_secs`, `channel_wait_secs`, `max_dumps`, `max_dump_mib` |
| `0x0003` | `GetHardware` (empty) | `Hardware`: `manufacturer`, `product`, `processor`, `virtualization`, `items`: `Item` {`location`, `kind` (open: `Disk` 1, `Interface` 2, `Pci` 3), `name`, `description`, `serial`, `bytes`, `mac`, `speed_mbps`, `carrier`, `owner` (open: `None` 1, `Host` 2, `Pool` 3, `Switch` 4, `Partition` 5, `Foreign` 6), `owner_name`, `partition`, `what` (the host's words)} |

| Event | Body | When |
|---|---|---|
| `0x0001` | `Host` | the resources changed: an activation, an end, a processor pool made, changed or deleted |
| `0x0002` | `ItemChanged` {`location`, `item` or none when gone, `time`} | an item's owner or carrier changed, or it came or went |

## partitions (`0x0011`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `List` (empty) | `Vec<Status>` in ID order: `id`, `name`, `state`, `code` and `text` (the current reference code), `current` (none when not activated), `profile` (last activated with), `last_end` (`exit N`, `signal N`), `message` (a `Note`: why it is not activated or in error), `ethernet`: `Link` {`slot`, `switch`, `mac`, `link`}, `channel` |
| `0x0002` | `Get` {`id`} | `Partition` {`definition`, `status`} |
| `0x0003` | `Adapters` {`id`, 0 for every partition} | `Vec<Adapter>`: `partition`, `name`, `profile`, `slot`, `switch`, `vlan`, `tagged`, `mac`, `required` |

| Event | Body | When |
|---|---|---|
| `0x0001` | `StateChanged` {`id`, `name`, `state`, `time`} | a state changed |
| `0x0002` | `ResourcesChanged` {`id`, `name`, `current`, `time`} | resources given at an activation or given back at an end |
| `0x0003` | `Defined` {`id`, `name`, `exists`, `time`} | created, changed or deleted |
| `0x0004` | `ChannelChanged` {`id`, `name`, `channel`, `time`} | the service channel came or went, stopped or started answering, or the system reported a change |

## partition (`0x0012`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `Create` {`name`} | `Definition`, with its ID and serial number |
| `0x0002` | `Change` {`definition`}: name, keylock, default profile, auto start; not activated | empty |
| `0x0003` | `Delete` {`id`, `disks`: its virtual disks too, every one or none} | empty |
| `0x0004` | `Activate` {`id`, `profile` (empty: the default), `source` override, `keylock` override} | `Current`: what it was given; it is Starting |
| `0x0005` | `ShutDown` {`id`, `kind` (open `ShutdownKind`: `Delayed` 1, `System` 2, `Immediate` 3), `delay` (s, 0 to 99999), `restart`} | empty: accepted by the system, or the VMM asked |
| `0x0006` | `Restart` {`id`}: immediate, activated again with the profile it had | empty |

No events: what follows comes as the partitions and refcodes families'.

## profiles (`0x0013`)

A profile has a **revision**, a number the managed side gives it and
changes whenever it changes. A put or save that carries a revision is
refused with `QSI0401` when the profile changed meanwhile; one that
carries none is refused with `QSI0402` when the name is there.

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `List` {`id`} | `Vec<Entry>`: `name`, `revision`, `default` |
| `0x0002` | `Get` {`id`, `name`} | `Revised` {`profile`, `revision`} |
| `0x0003` | `Put` {`id`, `profile`, `revision` or none for a new one} | `Saved` {`revision`} |
| `0x0004` | `Delete` {`id`, `name`}: not the default | empty |
| `0x0005` | `SetDefault` {`id`, `name`} | empty |
| `0x0006` | `SaveCurrent` {`id`, `name`, `revision` when replacing}: the running profile at the resources held now | `Saved` {`revision`} |

| Event | Body | When |
|---|---|---|
| `0x0001` | `Changed` {`id`, `name`, `revision` or none when deleted, `default`, `time`} | a profile was created, changed or deleted, or the default moved |

## panel (`0x0014`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `Function` {`id`, `function` (`u8`), `args`: `PanelArgs` {`source`, `ipl_type`, `keylock`}, each optional, function 02's} | `Shown` {`text`}: what it displays, the host's words |

The functions (`panel::function`): 01 display the IPL, 02 select it for
the next activation, 03 restart the IPL, 08 power off, 11 to 19 the
current code's words 1 to 9, 20 the type, serial and releases, 21
dedicated service tools (the system's, over the service channel), 22
dump and restart, 34 dump again from held memory. Which there are and
which need manual is the host's (`docs/PANEL.md` in QS/VS). No events.

## refcodes (`0x0015`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `History` {`id`, `before` (a place in the history, none from the newest), `limit`} | `Page` {`codes`: `Posted` {`refcode`: `base::Refcode` (`seq`, `code`, `words`, `text`, `time_ms`), `source` (open: `Host` 1, `System` 2)}, newest first; `more`: the next page's `before`} |

| Event | Body | When |
|---|---|---|
| `0x0001` | `Code` {`id`, `name`, `posted`, `current`: it is the current code now (a `C2009` note is not)} | a code was posted |

## dumps (`0x0016`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `List` {`id`} | `Vec<DumpInfo>` oldest first: `name`, `time`, `bytes`, `allocated` (in the pool), `code`, `reason` |
| `0x0002` | `Take` {`id`}: without restart | `Begun` {`name`}: written in the background |
| `0x0003` | `Delete` {`id`, `name`} | empty |
| `0x0004` | `Download` {`id`, `name`} | `Downloading` {`name`, `bytes`}: **opens a stream**, host to console, of the dump's `bytes` bytes (an ELF core), ending after the last |

| Event | Body | When |
|---|---|---|
| `0x0001` | `Taken` {`id`, `name`, `dump`, `error` (a `Note`) or none, `time`} | a dump was written or could not be |
| `0x0002` | `Removed` {`id`, `dump`, `time`} | a dump was deleted, or the oldest removed for the host's limits (`C2009060`) |

## console (`0x0017`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `Attach` {`id`, `take_over`} | `Attached` {`serial`}: **opens a stream** both ways |
| `0x0002` | `Screen` {`id`} | `Capture` {`serial`, `frame`: `qsipart::console::Frame` or none, `text`, a line a row} |

The stream is bytes without boundaries, so each value goes in a frame of `qsipart`'s header, kind `0x0401` (`console::PIECE`), read back with `console::Pieces`; a frame of another kind is passed over. From the host, `Out`:
`1` `Frame` (`qsipart::console::Frame`: the first after attaching, and
when the system sends one), `2` `Update` (`qsipart::console::Update`),
`3` `Serial` (bytes, when the profile's console is serial), `4` `Ended`
(why: taken over, the partition ended; the stream closes after it). From
the console, `In`: `1` `Key` (`qsipart::console::Key`), `2` `Refresh`,
`3` `Bytes` (for the serial port). A tag a later version adds is kept
as `Other(tag)` and the rest of its piece passed over. One viewer at a
time: an attach without `take_over` is refused while another views (the
host's `QVS2314`). Frames are checked with `qsipart`'s checks
(`Out::check`, `Capture::check`; an update against the last frame with
`qsipart::console::Grid`). Closing the stream detaches. No events.

## vres (`0x0018`)

| Op | Request | Reply |
|---|---|---|
| `0x0001` | `Pools` (empty) | `Vec<StoragePool>`: `name`, `uuid`, `protection`, `state`, `work`, `work_unit`, `percent`, `bytes`, `allocated`, `provisioned`, `repository`, `units`: `PoolUnit` {`number`, `disk`, `serial`, `location`, `bytes`, `state`}, `failure`. States and protections in the host's words |
| `0x0010` | `Vdisks` {`pool`, empty for every pool} | `Vec<Vdisk>`: `pool`, `name`, `bytes`, `allocated`, `thick`, `holder` |
| `0x0011` | `VdiskCreate` {`pool`, `name`, `mib`, `thick`} | `Vdisk` |
| `0x0012` | `VdiskResize` {`pool`, `name`, `mib`}: grow, not held | `Vdisk` |
| `0x0013` | `VdiskDelete` {`pool`, `name`, `force`: even when a profile names it} | empty |
| `0x0014` | `VdiskNamed` {`pool`, `name`} | `Vec<Naming>`: `partition`, `name`, `profile`, `slot`, `lun` |
| `0x0020` | `Images` (empty) | `Vec<Image>`: `name`, `bytes`, `label` (volume identifier), `holder` |
| `0x0021` | `ImageRename` {`name`, `to`} | empty |
| `0x0022` | `ImageDelete` {`name`} | empty |
| `0x0023` | `ImageUpload` {`name`, `bytes`} | `Uploading` {`name`, `bytes`}: **opens a stream**, console to host, of exactly `bytes` bytes; how it ended is the `Upload` event (more is `QSI0701`, fewer `QSI0702`, a refused image the host's message) |
| `0x0024` | `Drives` {`id`}: a running partition | `Vec<Drive>`: `slot`, `kind` (open: `Optical` 1, `Tape` 2), `name` (the medium, empty for none), `prevented` |
| `0x0025` | `Load` {`id`, `slot`, `name`}: load an image, or mount a tape volume | empty |
| `0x0026` | `Unload` {`id`, `slot`, `forced`}: unload, or unmount | `Unloaded` {`name`} |
| `0x0030` | `Volumes` (empty) | `Vec<Volume>`: `name`, `identifier`, `capacity_mib`, `used` (bytes), `protected`, `holder` |
| `0x0031` | `VolumeCreate` {`name`, `identifier`, `mib`} | `Volume` |
| `0x0032` | `VolumeDelete` {`name`} | empty |
| `0x0033` | `VolumeProtect` {`name`, `on`} | empty |
| `0x0040` | `Switches` (empty) | `Vec<Switch>`: `name`, `id`, `vlans`, `mtu`, `uplink`: `Uplink` {`interfaces` (location codes), `bond` (open: `ActiveBackup` 1, `Lacp` 2), `native_vlan`} or none, `uplink_state` (host's words), `members`: `Member` {`location`, `name`, `carrier`, `speed_mbps`}, `ports`, `failure` |
| `0x0041` | `Ports` {`switch`, empty for every one} | `Vec<Port>`: `partition`, `slot`, `switch`, `vlan`, `tagged`, `mac`, `link`, `counters`: `Counters` {`sent_bytes`, `sent_packets`, `sent_dropped`, `received_bytes`, `received_packets`, `received_dropped`} |
| `0x0042` | `Fdb` {`switch`} | `Vec<FdbRow>`: `mac`, `vlan`, `port`, `partition`, `slot`, `origin` |
| `0x0043` | `Vlans` (empty) | `Vec<VlanUse>`: `switch`, `vlan`, `native`, `ports`: `VlanPort` {`partition`, `slot`, `tagged`} |
| `0x0044` | `GetLimits` (empty) | `Limits`: `switches`, `vlans_per_switch`, `adapters_per_partition`, `ports_per_switch` |
| `0x0050` | `ProcPoolCreate` (`ProcPoolSpec` {`name`, `cores`, `maximum_units` (hundredths) or none}) | empty |
| `0x0051` | `ProcPoolChange` (`ProcPoolSpec`): no partition runs in it | empty |
| `0x0052` | `ProcPoolDelete` {`name`} | empty |

Storage pools and switches are read only: they are made at the host.
Shared processor pools are read in the host family's `Host`.

| Event | Body | When |
|---|---|---|
| `0x0001` | `PoolChanged` {`name`, `pool` or none, `time`} | a storage pool's state, work or allocation changed |
| `0x0002` | `VdiskChanged` {`pool`, `name`, `vdisk` or none, `time`} | made, grown, deleted, held or given back |
| `0x0003` | `ImageChanged` {`name`, `image` or none, `time`} | added, renamed (the old name gone, the new one added), deleted, loaded, unloaded |
| `0x0004` | `UploadEnded` {`name`, `image` or none, `error` (a `Note`) or none, `time`} | an upload or a host-side import ended |
| `0x0005` | `VolumeChanged` {`name`, `volume` or none, `time`} | made, deleted, protected, mounted, unmounted |
| `0x0006` | `DriveChanged` {`id`, `drive`, `time`} | a partition's device was loaded or unloaded, from a console or by its system |
| `0x0007` | `SwitchChanged` {`name`, `switch` or none, `time`} | a switch was made, changed or deleted |
| `0x0008` | `UplinkChanged` {`switch`, `location`, `carrier`, `state`, `time`} | an uplink interface lost or regained carrier |
| `0x0009` | `PortChanged` {`partition`, `slot`, `switch`, `open`, `time`} | a partition's port opened or closed |

## The families' message identifiers

`QSI03nn` to `QSI09nn` are the families'. A host's own refusals keep the
host's identifiers (`QVSnnnn`); these are for what QSI itself refuses.

| Range | Family | Defined in version 1 |
|---|---|---|
| `QSI0301`-`QSI0329` | host | `QSI0301` `SERVICE_DOWN`: the host's service that answers a request did not (its socket missing, no answer in time) |
| `QSI0330`-`QSI0359` | partitions | `QSI0331` `NO_PARTITION`: no partition of that ID, from a managed side with no identifier of its own (a QS/VS host answers `QVS2001`) |
| `QSI0360`-`QSI0399` | partition | `QSI0361` `NO_SUCH_VALUE`: an enumeration value this host does not have (a later version's shut down kind, IPL source, keylock) |
| `QSI0401`-`QSI0449` | profiles | `QSI0401` `CHANGED` (the revision conflict), `QSI0402` `EXISTS` (a name there, no revision), `QSI0403` `NOT_ACTIVE` (save current of a partition not activated) |
| `QSI0450`-`QSI0499` | panel | none |
| `QSI0501`-`QSI0549` | refcodes | none |
| `QSI0550`-`QSI0599` | dumps | none |
| `QSI0601`-`QSI0699` | console | none |
| `QSI0701`-`QSI0999` | vres | `QSI0701` `UPLOAD_LONG`, `QSI0702` `UPLOAD_CUT` |

## Left for later versions

Each will be a new capability bit (a new family, or a family's version
2):

- code update of the host (its PTFs, its A/B slots);
- serviceable events (the host's problems and their analysis);
- a host's own dumps (as against a partition's);
- physical I/O: giving a host's device to a partition (a profile carries
  `physical_io` already, so it reads whole);
- changing the host's settings, storage pools, switches, uplinks and the
  switches' limits from a console (made at the host in version 1);
- changing a running partition's resources (QS/VS has none yet).

## QS/VS's local protocols and their QSI messages

Every request of `part/partd/src/proto.rs` (34), `stg/poold/src/proto.rs`
(26) and `net/vswitchd/src/proto.rs` (17), by its name there, at QS/VS
commit `cebf3d2`: 77 requests, 53 with a QSI message and 24 with none.

### partd: 34 requests, 32 mapped, 2 none

| # | Request | QSI |
|---|---|---|
| 0 | `List` | partitions `0x0001` List |
| 1 | `Create` | partition `0x0001` Create |
| 2 | `GetDefinition` | partitions `0x0002` Get (its `definition`) |
| 3 | `Change` | partition `0x0002` Change; profiles `0x0005` SetDefault (a change of `default_profile`) |
| 4 | `Delete` | partition `0x0003` Delete |
| 5 | `ListProfiles` | profiles `0x0001` List (with revisions the QSI host server keeps) |
| 6 | `GetProfile` | profiles `0x0002` Get |
| 7 | `PutProfile` | profiles `0x0003` Put; profiles `0x0006` SaveCurrent (composed: the running profile at `Current`) |
| 8 | `DeleteProfile` | profiles `0x0004` Delete |
| 9 | `Activate` | partition `0x0004` Activate |
| 10 | `ShutDown` | partition `0x0005` ShutDown, `Immediate`, no restart |
| 11 | `Restart` | partition `0x0006` Restart (and `0x0005` `Immediate` with restart) |
| 12 | `Resources` | host `0x0001` Host; host event `0x0001` |
| 13 | `CreatePool` | vres `0x0050` ProcPoolCreate |
| 14 | `ChangePool` | vres `0x0051` ProcPoolChange |
| 15 | `DeletePool` | vres `0x0052` ProcPoolDelete |
| 16 | `History` | refcodes `0x0001` History (paged by the QSI host server) |
| 17 | `GetSettings` | host `0x0002` Settings |
| 18 | `ChangeSettings` | **none**: the host's settings are changed at the host in version 1 |
| 19 | `EndAll` | **none**: PID 1's at the host's shut down; a console reaches it through base `0x0003` Power |
| 20 | `Subscribe` | base `0x0005` Subscribe: `State`, `Resources`, `Defined`, `Channel` become partitions events `0x0001` to `0x0004`, `Code` refcodes `0x0001`, `Dump` dumps `0x0001` |
| 21 | `LoadMedia` | vres `0x0025` Load |
| 22 | `UnloadMedia` | vres `0x0026` Unload |
| 23 | `Media` | vres `0x0024` Drives |
| 24 | `Named` | vres `0x0014` VdiskNamed |
| 25 | `Adapters` | partitions `0x0003` Adapters |
| 26 | `ShutDownAs` | partition `0x0005` ShutDown |
| 27 | `Panel` | panel `0x0001` Function |
| 28 | `TakeDump` | dumps `0x0002` Take |
| 29 | `Dumps` | dumps `0x0001` List |
| 30 | `DeleteDump` | dumps `0x0003` Delete |
| 31 | `ReadDump` | dumps `0x0004` Download (its stream) |
| 32 | `Console` | console `0x0001` Attach (its stream) |
| 33 | `Screen` | console `0x0002` Screen |

### The pool service: 26 requests, 14 mapped, 12 none

| # | Request | QSI |
|---|---|---|
| 0 | `Disks` | host `0x0003` Hardware (the disk items, owned by a pool, foreign or free) |
| 1 | `Pools` | vres `0x0001` Pools |
| 2 | `CreatePool` | **none**: made at the host (it erases disks; a foreign disk is confirmed by typing its name) |
| 3 | `DeletePool` | **none**: at the host |
| 4 | `RenamePool` | **none**: at the host |
| 5 | `ReplaceUnit` | **none**: at the host, where the disk is |
| 6 | `AddSpare` | **none**: at the host |
| 7 | `RemoveSpare` | **none**: at the host |
| 8 | `Designate` | **none**: at the host |
| 9 | `Vdisks` | vres `0x0010` Vdisks |
| 10 | `CreateVdisk` | vres `0x0011` VdiskCreate |
| 11 | `ResizeVdisk` | vres `0x0012` VdiskResize |
| 12 | `DeleteVdisk` | vres `0x0013` VdiskDelete |
| 13 | `Images` | vres `0x0020` Images |
| 14 | `Upload` | vres `0x0023` ImageUpload (its stream; the second answer is vres event `0x0004`) |
| 15 | `RenameImage` | vres `0x0021` ImageRename |
| 16 | `DeleteImage` | vres `0x0022` ImageDelete |
| 17 | `ImportImage` | **none**: from the host's own removable volume or drive, at the host; its end is vres event `0x0004` |
| 18 | `Volumes` | vres `0x0030` Volumes |
| 19 | `CreateVolume` | vres `0x0031` VolumeCreate |
| 20 | `DeleteVolume` | vres `0x0032` VolumeDelete |
| 21 | `ProtectVolume` | vres `0x0033` VolumeProtect |
| 22 | `Hold` | **none**: internal, the partition manager's at activation (it passes open files) |
| 23 | `Release` | **none**: internal, the partition manager's |
| 24 | `Holders` | **none**: internal; each item's `holder` is in its list |
| 25 | `StopAll` | **none**: PID 1's at the host's shut down |

### The switch service: 17 requests, 7 mapped, 10 none

| # | Request | QSI |
|---|---|---|
| 0 | `Switches` | vres `0x0040` Switches |
| 1 | `Create` | **none**: made at the host |
| 2 | `Change` | **none**: at the host |
| 3 | `Delete` | **none**: at the host |
| 4 | `Rename` | **none**: at the host |
| 5 | `SetUplink` | **none**: at the host, where the cables are |
| 6 | `ClearUplink` | **none**: at the host |
| 7 | `Interfaces` | host `0x0003` Hardware (the interface items, owned by the host, a switch or none) |
| 8 | `Ports` | vres `0x0041` Ports |
| 9 | `Fdb` | vres `0x0042` Fdb |
| 10 | `Vlans` | vres `0x0043` Vlans |
| 11 | `OpenPort` | **none**: internal, the partition manager's at activation (it passes the tap) |
| 12 | `ClosePort` | **none**: internal |
| 13 | `GetLimits` | vres `0x0044` Limits |
| 14 | `ChangeLimits` | **none**: at the host |
| 15 | `Subscribe` | base `0x0005` Subscribe: `Switch`, `Uplink`, `Port` become vres events `0x0007` to `0x0009` |
| 16 | `Reconcile` | **none**: internal, the partition manager's when it starts |

### What QSI has that no local request gives

The QSI host server composes these:

- profile revisions (profiles `0x0001` to `0x0003`, `0x0006`, event
  `0x0001`): partd keeps none; the host server keeps one per profile and
  serialises puts so the check and the put are one step;
- profiles `0x0006` SaveCurrent: from the running profile (`Status.profile`)
  and `Current`, put with partd's `PutProfile`;
- the history's places and pages (refcodes `0x0001`): partd's `History`
  is the last 256, oldest first, without numbers;
- host `0x0003` Hardware: `inventory.toml` with the pool service's
  `Disks` and the switch service's `Interfaces` for each item's owner;
  host event `0x0002` from their changes;
- vres events `0x0001` to `0x0006`: the pool service has no
  subscription; the host server sends them for what it changed and what
  it finds when it polls (the partition manager polls the pools every
  two seconds already);
- dumps event `0x0002`: from `C2009060` in the history, and deletes.

Left out as the host's alone: a partition's VMM process (`Status.pid`),
a switch's bridge (`SwitchStatus.bridge`), a port's tap
(`PortStatus.tap`), a pool's mount point (`PoolStatus.mount`).

## Decisions taken in this draft

- **`Environment`**: QS/VS names its one environment by the product's
  name; this neutral crate cannot, so it is the open value `Standalone`
  (the operating system a standalone system runs). A host maps its word
  to it.
- **The keylock**: QS/VS has four positions; base's `Keylock` gained
  `Auto` 3 and `Secure` 4, additively.
- **`Current` flattened**: QS/VS's `Processors` enum has a tag a later
  mode would make unreadable; QSI's `Current` is a struct with an open
  `mode`.
- **Reference codes**: a partition's history uses base's `Refcode`
  (`time_ms`, `seq`) with a `source`, so a console handles a host's,
  a standalone system's and a partition's codes alike.
- **`VdiskNamed`** is structured (partition, profile, slot, LUN); partd's
  `Named` answers text (`PROD profile NORMAL slot 2`), which the host
  server reads.
- **Tape mount and unmount** are vres `Load` and `Unload` on a tape
  slot, as partd's `LoadMedia` and `UnloadMedia` are one for both.
- **`Restart`** is kept beside `ShutDown` `Immediate` with restart: both
  are partd's, and the handoff names it.
- **The console stream** carries the serial port's bytes too: a profile
  whose console is serial has no frames.

## Found in QS/VS that the handoff did not foresee

- A refused request answers with **every** message that applies, each
  with second-level help (`Vec<Issued>`); the session's refusal is one
  {msgid, text}. Either the session's refusal carries a list, or a host
  sends the first and loses the rest.
- partd has **no profile revisions** and no compare-and-put; the QSI
  host server keeps them.
- partd has **no request to save the current configuration** as a
  profile; it is composed.
- partd's history has **no sequence numbers**: places are the host
  server's.
- The **pool service has no events**; the switch service and partd do.
- An upload is answered **twice** by the pool service (`()` then the
  image); QSI's second answer is an event.
- partd's `Named` answers **text**, not structure.
- The console may be the **serial port** (`ConsoleOut::Serial`,
  `ConsoleIn::Bytes`), not only `qsipart`'s frames.
- QS/VS's keylock has **auto and secure**; base's had two positions.
- A profile carries **physical I/O** already (QS/VS's phase 10), before
  any family manages it.
- **Tape volume protection** (`ProtectVolume`) is a request the handoff
  did not list; vres has it (`0x0033`).

## Tests

`lic/qsi/tests/families.rs`: every sample round-trips, also with a later
version's trailing bytes; every operation of every family's `OPS` has a
request and a reply sample, and every event a sample; the families, their
numbers and capability bits agree with `caps::of` and `HOST_V1`; an
unknown operation or family decodes to none; every strict prefix of every
sample's body is an error; damaged bodies never panic; an open
enumeration keeps `Other` and passes it on; the samples are within the
bounds and values out of them are refused; the console streams
round-trip and keep a later kind. `qsi::samples` (`exchanges`, `events`,
`console_out`, `console_in`) is the session's tests' and the fuzzer's
seed set.
