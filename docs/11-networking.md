# 11 · Networking

Up to four players on the same Wi-Fi network, any mix of Macs and Windows PCs, one player hosting on their own machine. No servers, no accounts.

## Model: host-sequenced deterministic lockstep

Every machine runs the **same deterministic simulation** ([10 · Architecture](10-architecture.md#the-deterministic-simulation)). The only thing sent over the network is **player commands**. Because the simulation is identical everywhere, the same commands in the same order produce the same game on every machine, bit for bit, across Mac ARM and Windows x86 alike.

Why lockstep for an RTS:
- Bandwidth depends on how fast players click, not on how many soldiers exist. Thousands of soldiers cost nothing to synchronise.
- Replays, save games, rejoin and spectating come almost for free.
- It is what classic RTS games used, for the same reasons.

### The twist: the host sequences turns
Classic lockstep makes every player wait for every other player's input each turn, so one player's Wi-Fi hiccup freezes everyone. We avoid that:

1. Each tick is a **turn** (50 ms).
2. Clients send their commands to the host as soon as they're issued.
3. The **host assigns** each command to a future turn, `current turn + D` (the input delay), and broadcasts a **turn bundle** for every turn on a steady 20 Hz cadence, containing all commands from all players for that turn (usually empty).
4. Every machine simulates turn T when it holds bundle T.

The host never waits for clients. A client whose packet arrives late only has *its own* command land a turn later. A client that falls behind (slow CPU, bad Wi-Fi burst) catches up by simulating buffered turns slightly faster; only if a client falls far behind does the host gently slow the turn rate for everyone (time dilation) instead of freezing the game.

The host's own commands go through the same delay, so hosting gives no advantage.

### Input delay
- `D` adapts to measured conditions: `D = ceil((RTT/2 + 2 × jitter_p95 + 10 ms) / 50 ms)`, minimum 1 turn.
- On wired LAN: 1 turn (50 ms). On typical home Wi-Fi: 2 turns (100 ms). Re-evaluated continuously, changed only by one turn at a time with hysteresis.
- **Local feedback hides the delay:** the moment you click, you hear the unit acknowledge, see the move marker, the formation preview and the selection flash. Units start moving 50 to 100 ms later, which feels instant in an RTS.

## Transport

- Plain **UDP**, one socket per machine, packets kept under 1,200 bytes (safe on any network).
- Our own small reliability layer: every packet carries a sequence number, the latest received sequence and a 32-bit ack bitfield.
- **Redundancy instead of retransmission:** every packet repeats all data the other side hasn't acknowledged yet (the last few turn bundles, or unacknowledged commands). On Wi-Fi, where losses come in bursts, the next packet already contains what was lost, with no retransmit timers and no stalls. Turn bundles are tiny, so this is cheap.
- The host sends a bundle every 50 ms even when empty. This doubles as a keepalive that stops laptop Wi-Fi radios from dozing between packets.
- **Encrypted and authenticated:** X25519 key exchange at join, then ChaCha20-Poly1305 per packet. Cheap enough to always run, so there is one code path whether playing at home or over a virtual LAN.

### Bandwidth
| Direction | Typical | Busy late-game micro |
|---|---|---|
| Client → host | < 0.5 KB/s | ~2 KB/s |
| Host → each client | ~1 KB/s | ~3 KB/s |
| Host total (3 clients) | ~3 KB/s | ~10 KB/s |

Commands are compact binary (a move order for a 12-regiment selection is ~40 bytes: type, regiment ids as delta varints, quantised target, facing, frontage).

## Session flow

```
Host                         Client
 │  beacon (LAN broadcast + mDNS, every 1 s)
 │──────────────────────────────▶ appears in Join list
 │◀──── Hello (build hash, data hash, name, key share)
 │───── Welcome (slot, lobby state, key share) ───────▶
 │  lobby updates (faction, team, colour, ready) ⇄
 │───── Start (seed, settings, start turn) ───────────▶
 │◀──── Loaded ──────────────────────────────────────── (barrier)
 │───── Countdown 3·2·1, turn bundles begin ──────────▶
```

- **Discovery:** UDP broadcast beacon plus mDNS service `_grubcraft._udp`. Friends on the same network see the game without typing anything.
- **Direct join:** IP and port, or a short **join code** that encodes them (for virtual LANs and port-forwarding).
- **Version gate:** the Hello carries the build hash and the game-data hash. Mismatches are rejected with a clear message naming both versions.
- Optional lobby password.

## Desync detection and recovery

Determinism is engineered and tested ([10 · Architecture](10-architecture.md#testing-strategy)), but we plan for failure:

1. Every 10 turns each client sends the host a 64-bit hash of its simulation state.
2. On a mismatch, the host takes the majority hash as truth (the host's own state breaks ties).
3. Divergent clients receive a **snapshot** (300 KB to 1 MB, compressed) plus the commands since, load it, and continue. Players see a brief "Resyncing…" banner rather than a lost game.
4. Everyone writes a **desync report**: the last snapshot before divergence plus the input log. `gc_replay` can rerun that on both platforms and, in debug builds, compare per-system hashes tick by tick to find exactly which system diverged.

## Disconnects and rejoining

- A client silent for 3 s is marked disconnected. Lobby option: **Pause and wait** (default, up to 90 s) or **Continue**.
- A disconnected player's units do nothing new but still defend themselves by simulation rules. In team games, an ally can take control.
- **Rejoin:** the player relaunches and joins the same host. The host sends the latest snapshot and the commands since; the client fast-forwards. A tick costs 1 to 2 ms, so catching up on a minute of play takes about 2 to 3 seconds.
- **Host leaves:** at launch the game ends (with an automatic save so it can be resumed). **Host migration** is planned: since every client holds the full state, the client with the best connection becomes the new sequencer from the last turn everyone had, and the others reconnect to it.

## Save, load and replays

- **Multiplayer save:** the host saves a snapshot plus metadata. Loading it in a lobby lets players reclaim their slots and continue.
- **Replays** are the match header (build, data hash, settings, seed, players) plus the turn-by-turn command stream, delta-encoded and LZ4-compressed: about **50 to 150 KB per hour** of play. Every match is saved automatically.
- Seeking in long replays uses keyframe snapshots the viewer generates on first playback and caches locally.
- Replays are tied to the build and data hash they were recorded with. For a personal game that's acceptable; the replay browser marks incompatible ones.

## Playing beyond one Wi-Fi network

In order of effort:

1. **Now:** a virtual LAN tool such as Tailscale or ZeroTier. Friends install it once, then join by join code or IP. No code needed from us, and traffic is end-to-end encrypted by the tool as well as by us.
2. **Later:** port forwarding on the host's router, with a connection-test button in the Host screen.
3. **Later still:** UDP hole punching through a tiny rendezvous service so join codes work across the internet without extra software.

## Known trade-offs

- **Map hacking is possible** in lockstep, because every machine holds the full state. Among friends this is acceptable; we do not build anti-cheat.
- **Everyone runs the full simulation**, so the slowest machine sets the ceiling. That is why the simulation budget is tiny ([12 · Performance](12-performance.md)).
- **One build for everyone:** players must update together. The version gate makes this obvious rather than mysterious.
