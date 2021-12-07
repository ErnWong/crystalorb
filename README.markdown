| WARNING: This crate currently depends on nightly rust unstable and incomplete features. |
|---|

<div align="center">
  <h1>crystalorb</h1>
  <p><strong>Network-agnostic, high-level game networking library<br>for client-side prediction and server reconciliation.</strong></p>
  <p>
    <a href="https://crates.io/crates/crystalorb"><img alt="crates.io" src="https://meritbadge.herokuapp.com/crystalorb"></a>
    <a href="https://docs.rs/crystalorb"><img alt="docs.rs" src="https://docs.rs/crystalorb/badge.svg"></a>
    <a href="https://github.com/ErnWong/crystalorb/actions/workflows/ci.yml"><img alt="ci" src="https://github.com/ErnWong/crystalorb/actions/workflows/ci.yml/badge.svg"></a>
    <a href="https://github.com/ErnWong/crystalorb/actions/workflows/cd.yml"><img alt="cd" src="https://github.com/ErnWong/crystalorb/actions/workflows/cd.yml/badge.svg"></a>
    <a href="https://codecov.io/github/ErnWong/crystalorb?branch=master"><img alt="Coverage" src="https://codecov.io/github/ErnWong/crystalorb/coverage.svg?branch=master"></a>
  </p>
  <hr>
  <a href="https://ernestwong.nz/crystalorb/demo" title="Demo">
    <img src="https://github.com/ErnWong/crystalorb/blob/1a905001b8d3532a119e77c21a5ddd22e527fa6b/assets/screencapture.apng?raw=true">
  </a>
  <hr>
</div>

## Quick start

You may copy the [standalone](examples/standalone.rs) example to use as a starting template, and build off from there. You may also want to check out [crystalorb-mock-network](crates/crystalorb-mock-network) and [crystalorb-bevy-networking-turbulence](crates/crystalorb-bevy-networking-turbulence), either to use directly in your projects, or as examples for you to integrate your own choice of networking layer.

If you prefer a more visual and interactive example, there is the [demo shown above](https://ernestwong.nz/crystalorb/demo) that uses the [Rapier physics engine](https://rapier.rs). Feel free to use the demo's [source code](examples/demo) as a starting template for your next project.

For more information about how to implement the required traits, refer to the [docs](https://docs.rs/crystalorb).

## About

Game networking is hard, because we usually want to give the illusion that the physics simulation is responsive, fair, and consistent between multiple game clients despite having significant network latency between the clients and the server. There are many different ways to solve the game networking problem. CrystalOrb tries implementing one of such ways.

***CrystalOrb*** is a young networking library that implements:

- **Client-side prediction.** Clients immediately apply their local input to their simulation before waiting for the server, so that the player's inputs feel responsive.
- **Server reconciliation.** Server runs a delayed, authoritative version of the simulation, and periodically sends authoritative snapshots to each client. Since the server's snapshots represent an earlier simulation frame, each client fast-forwards the snapshot they receive until it matches the same timestamp as what's being shown on screen. Once the timestamps match, clients smoothly blend their states to the snapshot states.
- **Display state interpolation.** The simulation can run at a different time-step from the render framerate, and the client will automatically interpolate between the two simulation frames to get the render display state.

CrystalOrb **does not** (yet) implement "lag compensation" (depending on your definition of "lag compensation"), because CrystalOrb clients currently simulate all entities (including other players) in the *present* anyway. Some netcode clients would rather simulate entities in the *past* except for the local player. There are pros and cons to both methods:

- By simulating everything in the *present*, collision timings will be consistent across all clients provided that no player input significantly changes the course of the simulation. This might be beneficial for physics-heavy games like... Rocket League? This is what CrystalOrb does. If you have two clients side-by-side, and you issue a "jump" command for the player using the left client, then the right client will see a jump after a small delay, but both clients will see the player land at the exact same time.

- By simulating most remote entities in the *past*, remote entities require little correction in the client, so other players' movement will look more natural in response to their player inputs. If you have two clients side-by-side, and issue a "jump" command for the player using the left client, then the right client will not see any jerk in the movement, but the jump and the landing will occur slightly later after the left client.

Caveat: You need to bring your own physics engine and "mid-level" networking layer. CrystalOrb is only a "sequencer" of some sort, a library that encapsulates the high-level algorithm for synchronizing multiplayer physics games. CrystalOrb is physics-engine agnostic and networking agnostic (as long as you can implement the requested traits).

## Is CrystalOrb any good?

Doubt it. This is my first time doing game networking, so expect it to be all glitter and no actual gold. For more information on game networking, you might have better luck checking out the following:

- [Gaffer On Games articles](https://gafferongames.com/#posts).
- Gabriel Gambetta's [Fast-Paced Multiplayer](https://www.gabrielgambetta.com/client-server-game-architecture.html) series.
- The GDC Talk [It IS Rocket Science! The Physics of Rocket League Detailed](https://www.youtube.com/watch?v=ueEmiDM94IE) by Jared Cone.
- The GDC Talk [Overwatch Gameplay Architecture and Netcode](https://www.youtube.com/watch?v=W3aieHjyNvw) by Timothy Ford.
- An informational page in the [Networked Replication RFC](https://github.com/maniwani/rfcs/blob/main/replication_concepts.md) by @maniwani.

(Yes, those were where I absorbed most of my small game-networking knowledge from. Yes, their designs are probably much better than CrystalOrb)

## Similar and related projects

CrystalOrb is a client-server rollback library, and there are other kinds of rollback/networking libraries out there you should check out! Here are some other rollback libraries by other authors in the rust community you might be interested in:

- [GGRS](https://github.com/gschup/ggrs) by @gschup is a P2P rollback library based on the GGPO network SDK and has a [Bevy plugin](https://github.com/gschup/bevy_ggrs).
- [backroll-rs](https://github.com/HouraiTeahouse/backroll-rs) from @HouraiTeahouse is also a P2P rollback library based on the GGPO network SDK and also has a [Bevy plugin](https://github.com/HouraiTeahouse/backroll-rs/tree/main/bevy_backroll).
- [bevy_rollback](https://github.com/jamescarterbell/bevy_rollback) by @jamescarterbell is a rollback library made for the [Bevy game engine](https://bevyengine.org/).
- [evoke](https://github.com/arcana-engine/evoke) from @arcana-engine is a client-server state replication library with delta compression.
- [orb](https://github.com/smokku/soldank/tree/581b4f446b2cf5264f4c25f4cc2eaa1c0bfc192a/shared/src/orb) by @smokku looks into [extracting the fundamental part](https://github.com/ErnWong/crystalorb/pull/5#issuecomment-882757283) of CrystalOrb out into its own module.
- [snowglobe](https://github.com/hastearcade/snowglobe) from @hastearcade is a JavaScript port of CrystalOrb.
- The Bevy game engine has a [Networked Replication RFC](https://github.com/bevyengine/rfcs/pull/19) currently being drafted by @maniwani. It contains very [insightful information](https://github.com/maniwani/rfcs/blob/main/replication_concepts.md) about fast-paced game networking techniques (i.e. state replication).

<i><sub>Your project's not listed? Feel free to make a PR or [submit](https://github.com/ErnWong/crystalorb/issues/new/choose) an issue, and I'd be happy to promote it.</sub></i>

## More examples

- [Demo](https://github.com/ErnWong/crystalorb/blob/master/examples/demo) by @ErnWong is hard-coded for only two clients ([source](https://github.com/ErnWong/crystalorb/tree/master/examples/demo)).
- [orbgame](https://github.com/vilcans/orbgame) by @vilcans is a [Bevy](https://bevyengine.org/) demo that accepts more than two clients.
- [Dango Tribute](https://github.com/ErnWong/dango-tribute) by @ErnWong is a [Bevy](https://bevyengine.org/) interactive experience with multiplayer blob physics. CrystalOrb was the networking code that was eventually extracted out from that project.

<i><sub>Your project's not listed? Feel free to make a PR or [submit](https://github.com/ErnWong/crystalorb/issues/new/choose) an issue, and I'd be happy to promote it.</sub></i>

## Unstable Rust Features

CrystalOrb currently uses the following unstable features:

```rust
#![feature(const_fn_floating_point_arithmetic)]
#![feature(map_first_last)]
#![feature(adt_const_params)]
#![feature(generic_associated_types)]
```
