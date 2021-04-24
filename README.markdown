# Crystalorb

Game networking is hard, because we usually want to give the illusion that the physics simulation is responsive, fair, and consistent between multiple game clients despite having significant network latency between the clients and the server. There are many different ways to solve the game networking problem. Crystalorb tries implementing one of such ways.

***Crystalorb*** is a small networking library that implements:

- **Client-side prediction.** Clients immediately apply their local input to their simulation before waiting for the server, so that the player's inputs feel responsive.
- **Server reconciliation.** Server runs a delayed, authoritative version of the simulation, and periodically sends authoritative snapshots to each client. Each client fast-forwards the snapshot they receive until it matches the same timestamp as what's being shown on screen. Once the timestamps match, clients smoothly blend their states to the snapshot states.
- **Display state interpolation.** The simulation can run at a different time-step from the render framerate, and the client will automatically interpolate between the two simulation frames to get the render display state.

Crystalorb **does not** (yet) implement lag compensation (depending on your definition of "lag compensation"), because crystalorb clients currently simulate all entities (including other players) in the *present* anyway. Some netcode clients would rather simulate entities in the *past* except for the local player. There are pros and cons to both methods:

- By simulating everything in the *present*, collision timings will be consistent across all clients provided that no player input significantly changes the course of the simulation. This might be beneficial for physics-heavy games like... Rocket League? This is what crystalorb does. If you have two clients side-by-side, and you issue a "jump" command for the player using the left client, then the right client will see a jump after a small delay, but both clients will see the player land at the exact same time.

- By simulating most remote entities in the *past*, remote entities' state require little correction in the client, so other players' movement will look more natural in response to their player inputs. If you have two clients side-by-side, and issue a "jump" command for the player using the left client, then the right client will not see any jerk in the movement, but the jump and the landing will occur slightly later after the left client.

Caveat: You need to bring your own physics engine and "mid-level" networking layer. Crystalorb is only a "sequencer" of some sort, a library that encapsulates the high-level algorithm for synchronizing multiplayer physics games. Crystalorb is physics-engine agnostic and networking agnostic (as long as you can implement the requested traits).

## Is it good?

Doubt it. This is my first time doing game networking, so expect it to be all glitter and no actual gold. For more information on game networking, you might have better luck checking out the following:

- [Gaffer On Games articles](https://gafferongames.com/#posts).
- Gabriel Gambetta's [Fast-Paced Multiplayer](https://www.gabrielgambetta.com/client-server-game-architecture.html) series.
- The GDC Talk [It IS Rocket Science! The Physics of Rocket League Detailed](https://www.youtube.com/watch?v=ueEmiDM94IE) by Jared Cone.
- The GDC Talk [Overwatch Gameplay Architecture and Netcode] by Timothy Ford.

(Yes, those are where I absorbed most of my small game-networking knowledge from. Yes, their designs are probably much better than crystalorb)

## Unstable Rust Features

Crystalorb currently uses the following unstable features:

```rust
#![feature(const_fn_floating_point_arithmetic)]
#![feature(map_first_last)]
#![feature(const_generics)]
#![feature(generic_associated_types)]
```

## Demo

Using Bevy game engine + Rapier physics engine + Turbulence message channels + Naia sockets.

TODO: insert jif
