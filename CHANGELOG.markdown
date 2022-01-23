## Next

1. Updated all crates to rust 2021 edition. [#27](https://github.com/ErnWong/crystalorb/pull/27)
2. Updated `crystalorb-bevy-networking-turbulence` plugin crate to bevy 0.6. [#26](https://github.com/ErnWong/crystalorb/issues/26) [#27](https://github.com/ErnWong/crystalorb/pull/27)

## v0.3.0

### Breaking Changes

1. (Issue [#7](https://github.com/ErnWong/crystalorb/issues/7) | PR [#9](https://github.com/ErnWong/crystalorb/pull/9)). `NetworkResource::Connection::recv<MessageType>` has now been split (monomorphised) into three separate methods: `recv_command`, `recv_snapshot`, and `recv_clock_sync` to make it easier to implement a `NetworkResource`.

2. (Issue [#16](https://github.com/ErnWong/crystalorb/issues/16) | PRs [#17](https://github.com/ErnWong/crystalorb/pull/17), [#22](https://github.com/ErnWong/crystalorb/pull/22)). Update the rust nightly toolchain (nightly-2021-12-06).
  - The associated type `NetworkResource::ConnectionType` now has additional bounds as now required by the rust compiler (see [rust-lang/rust#87479](https://github.com/rust-lang/rust/issues/87479) and an insightful [rust forum post](https://users.rust-lang.org/t/associated-type-bounds-for-the-user-of-for-the-implementer/63161)). These bounds restrict how CrystalOrb can use `ConnectionType`, and so it should now be easier for your libraries to implement your own `NetworkResource` with fewer struggles with the borrow checker.

### Bugfixes

1. (Issue [#6](https://github.com/ErnWong/crystalorb/issues/6) | PR [#19](https://github.com/ErnWong/crystalorb/pull/19)). Fix a possible crash when the client sleeps for a long time and its state transitions from `ReconciliationStatus::Fastforwarding(FastforwardingHealth::Obsolete)` to `ReconciliationStatus::Blending(0.0)`.

## v0.2.1

### Fixed

- `crystalorb-bevy-networking-turbulence` plugin used the wrong settings resource for registering the `Timestamped<Snapshot>` message channel (it used `CommandChannelSettings` instead of `SnapshotChannelSettings`), causing both incorrect configuration as well as panic, since the same channel number will be used twice.

## v0.2.0

### Fixed

- Documentation and example codes in subcrate READMEs
- Previously, it was possible to disable the `serde/derive` feature of the `crystalorb` crate by specifying `default-features = false`. However, this is unintended since `crystalorb` does not compile without this feature. Therefore, `serde/derive` was moved from the default feature and into the dependency specification itself.
- The `crystalorb-bevy-networking-turbulence` crate did not even compile. This is now fixed.

### Added

- A bevy plugin implementation for the `crystalorb-bevy-networking-turbulence` crate.
