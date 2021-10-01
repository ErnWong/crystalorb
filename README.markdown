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

CrystalOrb **does not** (yet) implement "lag compensation" (depending on your definition of "lag compensation"), because crystalorb clients currently simulate all entities (including other players) in the *present* anyway. Some netcode clients would rather simulate entities in the *past* except for the local player. There are pros and cons to both methods:

- By simulating everything in the *present*, collision timings will be consistent across all clients provided that no player input significantly changes the course of the simulation. This might be beneficial for physics-heavy games like... Rocket League? This is what crystalorb does. If you have two clients side-by-side, and you issue a "jump" command for the player using the left client, then the right client will see a jump after a small delay, but both clients will see the player land at the exact same time.

- By simulating most remote entities in the *past*, remote entities require little correction in the client, so other players' movement will look more natural in response to their player inputs. If you have two clients side-by-side, and issue a "jump" command for the player using the left client, then the right client will not see any jerk in the movement, but the jump and the landing will occur slightly later after the left client.

Caveat: You need to bring your own physics engine and "mid-level" networking layer. CrystalOrb is only a "sequencer" of some sort, a library that encapsulates the high-level algorithm for synchronizing multiplayer physics games. CrystalOrb is physics-engine agnostic and networking agnostic (as long as you can implement the requested traits).

## Is crystalorb any good?

Doubt it. This is my first time doing game networking, so expect it to be all glitter and no actual gold. For more information on game networking, you might have better luck checking out the following:

- [Gaffer On Games articles](https://gafferongames.com/#posts).
- Gabriel Gambetta's [Fast-Paced Multiplayer](https://www.gabrielgambetta.com/client-server-game-architecture.html) series.
- The GDC Talk [It IS Rocket Science! The Physics of Rocket League Detailed](https://www.youtube.com/watch?v=ueEmiDM94IE) by Jared Cone.
- The GDC Talk [Overwatch Gameplay Architecture and Netcode](https://www.youtube.com/watch?v=W3aieHjyNvw) by Timothy Ford.

(Yes, those were where I absorbed most of my small game-networking knowledge from. Yes, their designs are probably much better than crystalorb)

## Unstable Rust Features

CrystalOrb currently uses the following unstable features:

```rust
#![feature(const_fn_floating_point_arithmetic)]
#![feature(map_first_last)]
#![feature(adt_const_params)]
#![feature(generic_associated_types)]
```
