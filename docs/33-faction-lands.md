# Faction lands

A faction's buildings change the ground around them. The Brood go first
(idea 19, built): where their root-veins reach, the soil turns to a living
skin of dark, wet flesh crossed by a network of glowing veins that pulse
towards the Hive. The Clans, Choir and Concord follow (idea 15, built), each
with a land of its own, drawn the same way.

## Why it is the ground and not objects

The root-veins used to be flat boxes laid along the ground: straight spokes
from each building and a thick link between buildings. They read as stripes
on the grass, not as a growth. The land is now drawn by the terrain shader,
per pixel, so it follows every slope, takes the same light, shadow and fog as
the ground and costs no instances.

## How it works

`gc_game::land` reads the simulation (the Brood's `World::veins`: each linked
Hive and Pustule, and how far its veins spread from its edge; the Choir's
`World::power_fields`; and the Clans' and Concord's finished buildings, below)
and, a few times a second, fills a grid for the renderer, four bytes a tile:

| Byte | What it holds |
|---|---|
| R | cover, 0 bare to 255 fully covered |
| G | whose land: a `LAND_*` kind (`gc_render`, shared with the shader) |
| B | distance to the nearest source in quarter tiles, up to 63.75 |
| A | spare |

Each source covers fully out to 70% of its reach from the building's edge and
thins to nothing at the reach. A building linked to another shown building
adds a corridor of land (2.6 tiles each side) between the two, so the network
is joined up even where the reaches do not meet. `Renderer::set_land` uploads
the grid as an `Rgba8Unorm` texture (binding 16, a tile each, 2.4 MB on a
768-tile map; one fetch a pixel, and the shader draws nothing where the cover
is under 0.05; where there is cover it gathers the four nearest kinds too).

The cover creeps: it grows 0.3 of a full cover a second when a building links
and fades at 0.6 a second when one is lost, so the land spreads and withdraws
instead of switching. The very first look at a map shows the land as it
stands, so loading a game does not replay the growth. Once the cover has
stopped moving and the sources are the same, nothing is recomputed or
uploaded, so a settled land costs nothing between changes. It runs on
wall-clock time, so it also keeps growing while the game is paused.

What a side sees follows the fog: only the buildings `shown_to` that side
(the same rule as the building meshes) are sources, so enemy land shows when
you have seen its buildings and not before; `reveal_all` shows all of it. The
whole thing is presentation: it reads the world and writes nothing to it, so
it cannot affect determinism.

## The Brood's look (`brood_land` in `world.wgsl`)

- **Skin:** two dark reds mottled by a slow noise and darkened by a fine one,
  rough 0.45 so it looks wet; soft swellings between the veins tilt the normal.
- **Veins:** the creases of two scales of noise, warped so they wander; thick
  near the source and thinner far out. The vein is darker than the skin, tilts
  the normal to sit in a groove, and glows crimson.
- **Pulses:** the glow brightens in bands running towards smaller distances,
  that is, inwards to the Hive, each band a little off the others' time by
  place. Time is `g.misc.y`.
- **Edge:** for every land (`faction_land`), the cover is wobbled by noise so
  the border is ragged, not a disc.
- Rock and cliff keep their own look, with only the glow added; water draws
  over the land as it does over the ground.

What still moves on the veins is drawn as boxes in `buildings::push_veins`:
the bright flow of Grub from a worked seep home, and the dull stub and red
cross on a seep the veins do not reach. They sit on top of the land.

## The other three lands (idea 15, built)

The grid's green byte says whose land a tile is (`LAND_BROOD`, `LAND_CLANS`,
`LAND_CHOIR`, `LAND_CONCORD`: 1 to 4, defined in `gc_render` and put in front of
the shader by `world_source`). The shader reads it without blending (the
biggest of the four nearest tiles, with `textureGather`) and `faction_land`
picks the land. Where two lands overlap, the one with more cover takes the tile,
and a fading tile keeps the look of the land it had.

| Faction | Spreads from | Look |
|---|---|---|
| Ember Clans | every finished building except walls and towers: 10 tiles from a Hall, 6 from an outpost, 2.5 from the rest | tilled earth: fields of about six tiles with furrows turned their own way, between yards of trodden earth |
| Shard Choir | each finished powering building's field of power (`World::power_fields`: Spire 8, Resonator 6) | frosted lavender ground with pale cracks, glinting crystal dust and faint rings that spread out from the source |
| Carrion Concord | the same as the Clans | grey ash, dried and split, with flecks of bone |

Walls, towers and unfinished buildings spread no land (Clans and Concord), so
land follows the settled base, not the siege line. Only buildings the viewing
side is shown count, as for the Brood.

Not built: land for tilled fields that grow crops, grave markers, and shards
standing out of the ground. The ground changes of clearing, burning and
flooding (docs/32) are separate; when they land they can write the same grid
or a sibling.

## Tuning

The numbers are constants at the top of `land.rs` (growth and fade speed,
corridor width, and the Clans' and Concord's reaches) and in each land's
function (`brood_land`, `clans_land`, `choir_land`, `concord_land`: colours,
widths, glow strength). The Brood's and the Choir's reach is the simulation's
(docs/21: Hive 15 tiles, Pustule 8; Spire 8, Resonator 6); the Clans' and
Concord's is for looks only, so it is not in the data or the replay.
