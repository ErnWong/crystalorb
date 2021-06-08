## v0.2.0

### Fixed

- Documentation and example codes in subcrate READMEs
- Previously, it was possible to disable the `serde/derive` feature of the `crystalorb` crate by specifying `default-features = false`. However, this is unintended since `crystalorb` does not compile without this feature. Therefore, `serde/derive` was moved from the default feature and into the dependency specification itself.
- The `crystalorb-bevy-networking-turbulence` crate did not even compile. This is now fixed.

### Added

- A bevy plugin implementation for the `crystalorb-bevy-networking-turbulence` crate.
