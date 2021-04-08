# Crystalorb

Crystalorb is an opinionated, high-level game networking library that sequences the following activities for you:
- Runs the server's physics simulation later than the clients, so that all of the needed player commands arrive in time for the server's next frame.
- Schedule and replay player commands at the right time even when they arrive out of order.
- Sending server's authoritative snapshots to the clients periodically.
- Extrapolates the server snapshots to the current time on the client using the client's command buffers
- Smoothly interpolates between the client's current physics state and the extrapolated server's snapshot when it arrives, so objects don't simply teleport.
- Runs the physics simulation in fixed time-steps, and interpolates between the last two time-steps for rendering at a different refresh rate.
- Limits the number of physics time-steps that could be advanced on each update call to prevent freezing.
- Limits how far the server snapshot can be extrapolated on each update call to prevent freezing.
- Jumps to the correct time-step if the simulation has fallen too far behind.
- Periodically synchronizes the client clock with the server clock.

Caveat: You need to bring your own physics engine and mid-level networking layer. Crystalorb is only a "sequencer", a library that encapsulates the high-level algorithm for synchronising multiplayer physics games. Crystalorb is physics-engine agnostic and networking agnostic (as long as you can implement the requested traits).

## Is it good?

Doubt it. This is my first time doing game networking, so expect it to be all glitter and no actual gold. However, this is inspired by Gaffer On Games articles.

## Demo

Using Bevy game engine + Rapier physics engine + Turbulence message channels + Naia sockets.

TODO: insert jif
