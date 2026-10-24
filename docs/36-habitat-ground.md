# 36 · Habitat ground

Each of a grown map's 19 habitats ([27 · Living land](27-living-land.md)) now
has ground of its own, not just a tint by land: bog with pools of black water,
sodden mud under alders, drifts of flowers in a meadow, purple heather,
gravel on a scree, crazed crust and combed ripples of bone dust in the waste,
leaf litter and moss under old trees. This is the ground-materials half of
Living Land's stage 4 (the scenery half is [25](25-scenery-models.md)). It is
render only: `gc_sim` and replays never read it.

## How it works

`gc_game::ground_look` turns `Map::habitat` into a grid for the renderer, once
when the map is loaded, four bytes a tile (`Terrain::look`):

| Byte | What it holds |
|---|---|
| R, G, B | what the grass colour is multiplied by (128 leaves it as it is, 255 doubles it) |
| A | a pattern drawn over the grass: one of the `GROUND_*` kinds in `gc_render`, 0 for none |

A map with no habitats (painted maps, the sandbox) hands over nothing and
keeps the plain ground. `gc_render` uploads the grid as an `Rgba8Unorm`
texture (`ground_look`, binding 18, a tile each, 2.4 MB on a 768 map; one
texel for a map with none).

In the terrain shader the colour is read with a bilinear fetch, so the grass
shifts gently from habitat to habitat, and the pattern is read from the
nearest tile. Both look up at a position moved by two scales of noise (up to
about two tiles), so the border between habitats is ragged, not tile-shaped.
`habitat_ground` in `world.wgsl` draws the pattern from the position alone,
with no textures, and the fine detail fades with distance. It goes over the
palette and the grass texture, under the ground marks ([35](35-ground-marks.md))
and a faction's land ([33](33-faction-lands.md)), so a worn track or a Brood
land still shows on top, and none of it shows on rock.

| Habitat | Colour | Pattern |
|---|---|---|
| Flower meadow | warmer, brighter | drifts of yellow, white and pink flowers |
| Grassland, scrub edge | plain, scrub a little browner | none |
| Wet meadow | cool and dark | damp, darker patches |
| Old growth | dark | leaf litter and moss |
| Birch fringe, glade | lighter | none |
| Fern hollow | deep green | moss |
| Sedge tussock | olive and dry-looking | none |
| Alder carr | dark | mud, glossy where wet |
| Peat bog | dark | hummocks of peat between pools of black water, glossy |
| Heather moor | greyed | purple heather over dry grass |
| Scree, bare rock | plain | grey gravel and pebbles |
| Tree line | dim | leaf litter and moss |
| Cracked flat | plain | pale crust crazed with dark cracks |
| Bone drift | plain | pale ripples of bone dust |
| Rib field | plain | pale bone dust |
| Dying grass | straw | patches of straw |

Standing water (the bog's pools, the mud) is drawn glossy: the same shine the
faction lands use (`shade_material` with a low roughness), so it mirrors the
sky a little when the light is low.

## Reading soft ground

The terrain tactics work ([32](32-changing-land.md), idea 13) will make peat
bog and alder carr soft footing (troops slowed, charges stopped), while sedge
tussock stays firm. The two soft habitats are the two with the darkest, wettest
patterns (bog pools, mud; wet meadow has only a faint damp), and sedge is drawn plain and dry, so players can see
where cavalry will bog down. When `Map::footing` lands, `ground_look` can take
its soft tiles for the same pattern on painted fens.

## Cost

A map without habitats gets a one-texel texture and the shader skips all of
it. Otherwise one more texture binding, four noise reads for the wobble, one
bilinear fetch and one load a pixel on ground, plus a pattern worked out from the position on tiles that have one (a
few noise reads; fbm costs four). The soldiers' pipeline layout is now 18
sampled textures, which the raised device limit allows
([35](35-ground-marks.md#cost)).

## Not yet

- Edges blend by ragged cutoff, not by a true weight between two patterns.
- Habitats on painted maps (the Wilds), which have none.
- Scenery's own use of the habitat (placing models by habitat is Living Land
  stage 4's other half).
