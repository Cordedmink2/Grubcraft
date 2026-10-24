# 35 · Ground marks

The ground remembers what was done on it. Where workers have felled trees the
earth is turned and dark, and where regiments have marched or fought the grass
is trodden into bare tracks that heal over a few minutes. This is idea 2 (the
world that remembers, render side) and the ground half of idea 22 (clearing
land, [32](32-changing-land.md)). Where the Concord burns a tile
clear ([32](32-changing-land.md#faction-twists)) the ground is scorched. Felled trees also leave stumps ([25](25-scenery-models.md#stumps)).

## What is drawn

`gc_game::scars` fills a grid for the renderer, four bytes a tile:

| Byte | What it holds |
|---|---|
| R | felled: turned earth where trees stood (150 for scrub, 255 for woods and thicket) |
| G | scorched: char and ash where the Concord burnt the tile (255 when `Event::Cleared` says `burnt`) |
| B | worn: trodden bare, 0 grass to 255 bare dirt |
| A | soaked: mud where rain has soaked the ground soft (255 for each of `Terrain::soaked_tiles`, refreshed when `Terrain::soaked` changes; drawn with the soft ground's mud, [32](32-changing-land.md#rain-soaks-the-ground-stage-2)) |

`Renderer::set_scars` uploads it as an `Rgba8Unorm` texture (binding 17, a
tile each, like the land grid; `None` clears it). The terrain shader reads it
with one fetch a pixel and does nothing where the three channels are under
0.03. Where there is a mark, noise moves its edge so it is ragged rather than
tile-shaped, and the albedo goes toward bare dirt (worn), dark loam (felled) or
char with drifts of grey ash (scorched), with grit; turned and trodden earth also lie in small clods
(the normal is tilted). None of it shows on rock. Land (the Brood's flesh,
tilled Clans fields and so on) draws over marks: a faction's own ground is not
trodden bare.

## Where marks come from

**Felled ground.** `Running::fell_scenery` already hears `Event::Cleared` each
tick to take the trees off; it now hands the same events to
`ScarView::felled`, which sets the tile's R by what stood there (`was`), and its G to 255 if the event
says the tile was `burnt`.
Replays and network games tick a world the same way, so they show it too.

**Worn ground.** A few times a second (every 250 ms) each regiment that is
marching (not standing or turning on the spot) adds a little wear (0.05) and
each that is in a fight a good deal more (0.15), both scaled by game speed and
nothing while paused, in a soft circle round it, a little wider for a bigger
regiment (`0.6 + 0.12 * sqrt(models)` tiles). Roads and favourite routes therefore wear
in over a few passes and a battlefield is churned quickly. Wear heals at 0.004
a second (about four minutes from fully bare, on the clock), in batches every five seconds
so the whole map is not walked each frame. A grid that has gone back to bare
is released (`set_scars(None)`).

It is presentation only, like the land: it reads the world and writes
nothing, nothing is hashed, and it is not part of any replay. It follows the
fog: only regiments the viewing side `sees` wear the ground (`reveal_all`
shows all), so tracks do not reveal an enemy march. Healing runs on wall-clock time
(like the land's growth), so a paused game still heals; building wear goes by game speed instead.

## Cost

One more texture binding (a tile each; 2.4 MB on a 768-tile map), one extra
fetch a pixel, a cheap early-out where nothing is marked, and a small CPU
loop over the regiments four times a second. While regiments march in view
the whole grid is sent each time it changes (up to 2.4 MB four times a
second on the largest map); a settled map sends nothing. The soldiers'
pipeline layout (the ground bindings plus its materials) is now 17 sampled
textures, past the 16 a baseline GPU allows, so the renderer asks for the
adapter's own limit (every desktop GPU we target allows 128 or more).

## Not yet

- Scorch that outlives a replay seek or a rejoin: like the other marks it is
  built from the clearing events a machine sees, and the event is the only
  place the burning is kept ([32](32-changing-land.md)).
- Burnt grass tufts: the tufts on a scorched tile still stand green.
- Moving scenery onto the shared detail textures.
- Churn and blood for fights beyond bare wear.
