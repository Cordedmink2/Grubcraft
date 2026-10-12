# Glossary

| Term | Meaning |
|---|---|
| **Army tray** | The row of regiment cards at the bottom of the screen |
| **Bloodseep** | A rich, temporary seep opened by a Titan Tremor |
| **Chord** | A Choir triangle of three linked Resonators; boosts seeps inside it |
| **Commander View** | Far-zoom view where regiments are drawn as banner blocks instead of soldiers |
| **Conquest** | The primary mode: base building and war |
| **Creep camp** | Neutral Titan-spawn monsters guarding resources; give experience and items |
| **Desync** | When two machines' simulations diverge; detected by state hashes |
| **Difficulty** | How well an AI player thinks (Recruit, Veteran, General, Warlord); never cheats. See also *Handicap* |
| **Doctrine** | A battle-plan template an AI chooses from, such as Hold the line or Hammer and anvil |
| **Dregs** | A depleted seep or vein, still yielding 25% |
| **Edict** | An Ember Clans global ability bought with Honor |
| **Faction meter** | Each faction's single extra resource: Honor, Larvae, Resonance, Tithe |
| **Field Battle** | The secondary mode: pre-built armies, one battle |
| **Fixed-point** | Integer-based fractional numbers used in the simulation for cross-platform determinism |
| **Flow field** | A grid of directions toward a destination, shared by many units |
| **Grub** | The common resource: the glowing ooze the Titan's body weeps |
| **Grubfolk** | Neutral farmer people living in hamlets |
| **Hamlet** | A neutral Grubfolk settlement: trade, hire, or raid |
| **Handicap** | A lobby setting for any slot, human or AI: income from 50% to 150% |
| **Liking, trust, fear** | How one party feels about another: liking and trust are made of fading memories of what was done; fear is worked out from nearby strength |
| **Lockstep** | Networking model where only commands are sent and every machine simulates identically |
| **Marrow** | The rare resource: crystal from the Titan's bones |
| **Mercenary company** | A band of fighters with a captain and a price that serves whoever pays |
| **Metamorphosis** | Brood ability to evolve a regiment into another form in the field |
| **Mind** | The AI that makes choices for a party (an AI player, a mercenary company, a faction); runs on the host and acts only through commands |
| **Model** | One soldier within a regiment |
| **Pack** | The single memory-mapped asset file, `grubcraft.pak` |
| **Parley** | The panel for dealing with another party: trade, hire, gifts, with each answer shown before you commit |
| **Party** | Anyone who can own units: a player (human or AI), a hamlet, a creep camp, a mercenary company, a faction |
| **Personality** | An AI's traits (aggression, caution, greed, honour and so on), preferred doctrines and openings |
| **Presentation layer** | Non-deterministic local code that turns regiment state into animated soldiers |
| **Pustule** | Brood relay node for root-veins; also housing |
| **Reach, the** | The lands built on the Titan's corpse |
| **Refraction** | Choir rule: beams pass through allied crystal units and split |
| **Regiment** | A block of models commanded as one unit |
| **Rehearsal** | The top AI difficulty playing candidate battle plans ahead in a copy of the real simulation |
| **Relations ledger** | The sim's record of how every party stands with every other: stance, liking, trust and memories |
| **Reliquary** | Concord structure that collects Tithe from nearby deaths |
| **Resonator** | Choir pylon that tunes seeps, links chords and provides supply |
| **Root-veins** | Brood network that carries Grub from burrowed workers to the Hive |
| **Seep** | A Grub source on the map |
| **Shrine** | Capturable neutral point giving vision and a small aura |
| **Stance** | What a party's units do about another party: Hostile, Wary, Neutral, Friendly, Allied (or Serving, for hired mercenaries) |
| **Tempo** | Lobby setting that scales gather and build speed |
| **Titan** | The dead god whose body is the map |
| **Titan's Heart** | Central objective; hold it for 3 minutes to win |
| **Turn** | One 50 ms lockstep step; equal to one simulation tick |
| **Upkeep** | Income penalty at high supply |
| **Vein (Marrow)** | A Marrow source on the map (distinct from Brood root-veins) |
