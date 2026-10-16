# Glossary

| Term | Meaning |
|---|---|
| **Ambush** | A hidden regiment striking an enemy whose side hadn't seen it until moments before: the victim loses 20 morale and is caught off guard. See [20](20-ambushes.md) |
| **Army tray** | The row of regiment cards at the bottom of the screen |
| **Bloodseep** | A rich, temporary seep opened by a Titan Tremor |
| **Campaign** | The planned third mode: a world map of the Reach played over several game nights, with seasons, hosts and the peoples you meet. See [19](19-campaign.md) |
| **Caught off guard** | An ambush's victim for 6 seconds afterwards: its blows deal 70% damage |
| **Chord** | A Choir triangle of three linked Resonators; boosts seeps inside it |
| **Chronicle** | The campaign's history: one line per thing that mattered, each battle linked to its replay |
| **Commander View** | Far-zoom view where regiments are drawn as banner blocks instead of soldiers |
| **Conquest** | The primary mode: base building and war |
| **Cover** | Woods and tall grass, one byte a tile on the map; regiments standing in it can be hidden |
| **Creep camp** | Neutral Titan-spawn monsters guarding resources; give experience and items |
| **Desync** | When two machines' simulations diverge; detected by state hashes |
| **Difficulty** | How well an AI player thinks (Recruit, Veteran, General, Warlord); never cheats. See also *Handicap* |
| **Doctrine** | A battle-plan template an AI chooses from, such as Hold the line or Hammer and anvil |
| **Dregs** | A depleted seep or vein, still yielding 25% |
| **Edict** | An Ember Clans global ability bought with Honor |
| **Encounter card** | A campaign site or meeting shown as up to three choices, each with its cost and result visible before you pick |
| **Faction meter** | Each faction's single extra resource: Honor, Larvae, Resonance, Tithe |
| **Field Battle** | The secondary mode: pre-built armies, one battle |
| **First contact** | The first time one of your units sees a party; a ledger event that moves its stance to you from Unknown to its starting stance |
| **Fixed-point** | Integer-based fractional numbers used in the simulation for cross-platform determinism |
| **Flow field** | A grid of directions toward a destination, shared by many units |
| **Frontier site** | A place hidden in the fog of a Conquest map (ruin, lost caravan, relic barrow and so on) that rewards whoever scouts it first |
| **Grub** | The common resource: the glowing ooze the Titan's body weeps |
| **Grubfolk** | Neutral farmer people living in hamlets |
| **Guest commander** | A player not in a campaign battle who takes command of one of its other sides (an ally's host, a company, a settlement's militia) |
| **Hamlet** | A neutral Grubfolk settlement: trade, hire, or raid |
| **Handicap** | A lobby setting for any slot, human or AI: income from 50% to 150% |
| **Hidden** | A regiment in cover that no enemy has spotted: enemies can't see, target or order attacks on it. Yours carry a moss-green flag |
| **Host** | An army on the campaign map, led by a hero; up to three per player |
| **Let the generals fight** | Resolving a campaign battle by having the battle AI command every side in the real simulation at full speed |
| **Liking, trust, fear** | How one party feels about another: liking and trust are made of fading memories of what was done; fear is worked out from nearby strength |
| **Lockstep** | Networking model where only commands are sent and every machine simulates identically |
| **Marrow** | The rare resource: crystal from the Titan's bones |
| **Mercenary company** | A band of fighters with a captain and a price that serves whoever pays |
| **Metamorphosis** | Brood ability to evolve a regiment into another form in the field |
| **Mind** | The AI that makes choices for a party (an AI player, a mercenary company, a faction); runs on the host and acts only through commands |
| **Model** | One soldier within a regiment |
| **Pack** | A baked asset file: `assets/models.pak` and `assets/audio/sounds.gcsnd`, each with a manifest, both compiled into the executable. The plan is one memory-mapped `grubcraft.pak` |
| **Parley** | The panel for dealing with another party: trade, hire, gifts, with each answer shown before you commit |
| **Party** | Anyone who can own units: a player (human or AI), a hamlet, a creep camp, a mercenary company, a faction |
| **Pathfinder** | A campaign scout: fast, can't fight, reveals regions |
| **Personality** | An AI's traits (aggression, caution, greed, honour and so on), preferred doctrines and openings |
| **Presentation layer** | Non-deterministic local code that turns regiment state into animated soldiers |
| **Pustule** | Brood relay node for root-veins; also housing |
| **Reach, the** | The lands built on the Titan's corpse |
| **Refraction** | Choir rule: beams pass through allied crystal units and split |
| **Regiment** | A block of models commanded as one unit |
| **Region** | One area of the campaign map: a ground type, a yield, at most one feature and one Work |
| **Rehearsal** | The top AI difficulty playing candidate battle plans ahead in a copy of the real simulation |
| **Relations ledger** | The sim's record of how every party stands with every other: stance, liking, trust and memories |
| **Reliquary** | Concord structure that collects Tithe from nearby deaths |
| **Resonator** | Choir pylon that tunes seeps, links chords and provides supply |
| **Root-veins** | Brood network that carries Grub from burrowed workers to the Hive |
| **Season** | One campaign turn: plan, resolve, battles, then the world moves on. (A *turn* is the 50 ms simulation step) |
| **Seat** | A faction's home region and capital on the campaign map |
| **Seep** | A Grub source on the map |
| **Shrine** | Capturable neutral point giving vision and a small aura |
| **Spotting** | Seeing a hidden regiment by coming close: within 4 tiles of a still regiment in a wood (10 if it moves), 3 in tall grass; what one regiment spots, its whole side sees |
| **Stance** | What a party's units do about another party: Hostile, Wary, Neutral, Friendly, Allied (or Serving, for hired mercenaries) |
| **Tempo** | Lobby setting that scales gather and build speed |
| **Titan** | The dead god whose body is the map |
| **Titan's Heart** | Central objective; hold it for 3 minutes to win |
| **Turn** | One 50 ms lockstep step; equal to one simulation tick |
| **Upkeep** | Income penalty at high supply |
| **Vein (Marrow)** | A Marrow source on the map (distinct from Brood root-veins) |
| **Work** | The one thing built in a held campaign region: Granary, Mine, Watchtower, Road or Fort |
