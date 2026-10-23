# 31 · Surface detail

*Idea 21, parts 1 and 4 (Mario, 2026-10-23, "less flat textures, at minimal cost"). Part 1 is a small set of repeating detail textures shared by every building and unit; part 4 is a stronger bake. Part 2 (ground textures) and part 5 (the same detail on scenery) belong to the Living ground thread, which reuses what is described here. See the [restart plan](29-restart-plan.md).*

## The problem

A building's baked atlas is 512 × 512 texels for a whole 5 × 5 building (bakes are made at 1024 and shrunk), so close up the stone, timber and cloth are soft and smooth. More atlas resolution would cost memory in every pack (a material is about 1 MB today). Repeating detail costs almost nothing: fifteen small textures serve everything.

## How it works

- **Fifteen surface classes** ([`gc_assets::detail`](../crates/gc_assets/src/detail.rs)): stone, brick, wood, planks, plaster, tile, cloth, leather, iron, bronze, bone, chitin, organic, crystal, earth. Each is two 256 × 256 layers, BC7-compressed with mips, 2.5 MB for all in `assets/detail.pak`. The first layer holds a colour change (grey 0.5 leaves the baked colour alone, the shader multiplies by twice the value) and a cavity (1 open, lower darkens); the second holds a normal's x and y, a roughness change and a height (not read yet; it is there for blending classes by height). The relief layer's mips are averaged as plain values, so its tilt flattens with distance. Both are linear.
- **Each face says what it wears.** A model's vertices carry two classes (four bits each), a mix between them, and how many times the detail repeats across the model's atlas (a four-bit code, 2 to about 584, half again more each step). They live in the two padding bytes of `ModelVertex` (the normal's fourth byte, and the colour's alpha), so the vertex stays 32 bytes, the model schema stays 4, and every pack baked before detail existed still loads and simply has none (zero classes).
- **The baker works the repeat out.** `gc_artbake` takes the area-weighted median over a model's triangles of the metres of surface per unit of atlas, divides by the model's `detail_tile` (metres per repeat, 0.5 for buildings) and writes the nearest code. The median keeps the shrunken plinth faces below the ground from deciding it.
- **The shader layers it** (`fs_soldier_textured`, `world.wgsl`): it samples the class's two layers at the atlas coordinate times the repeat (with the atlas's own derivatives; mips flatten it with distance, and the shader fades it out and skips the samples once a pixel covers more than 1/8 of a repeat), multiplies the colour, adds the detail normal's x and y to the baked normal's, shifts the roughness and multiplies the occlusion by the cavity. A second class is blended in by the mix. Detail is applied to buildings and textured soldiers alike. A GPU without BC7 (the same ones that fall back to vertex colours) binds a texture of neutral texels, so nothing changes.
- **Sources and generator.** `assets-src/detail/<class>_d.png` and `<class>_n.png` are made by `python3 -I scripts/gen_detail.py` (numpy and Pillow; procedural, seamless, deterministic, so no licence question). `cargo run --release -p gc_artbake -- --detail` bakes `assets/detail.pak` and its manifest; CI runs it with `--check`. To change a class, edit the generator, rerun both, commit the images, pack and manifest together.
- **A model asks for detail** with `detail_tile: Some(0.5)` in its `assets-src` config entry; its glTF carries each face's classes as `COLOR_0` (red and green are the two classes in fifteenths, blue the mix). Models without `detail_tile` ignore `COLOR_0` (some carry real vertex colours).

## Which class a material wears

The building baker (`design/buildings/source/surfaces.py` in the shared folder) picks it from the procedural material, so no building script names one: the bump kind (`bricks`, `tiles`, `planks`, `grain`, `cloth`), then fragments of the material's name for the plain `noise` bump (plaster, bone, verdigris, iron, carapace, flesh, frost, earth, stone), and glowing materials (lamps, windows, veins) wear none. `bkit.mat()` stores the bump kind on the material for it. `CLASSES` there and `DetailClass` in `gc_assets` must stay in step.

## The stronger bake (idea 21 part 4)

`design/buildings/source/bake.py` (the one before it is kept as `bake_v1.py`) now:

- darkens crevices: a second, short-range occlusion pass (0.14 tiles) multiplies into the occlusion map and dirties the colour where seams meet (`CREVICE_*`);
- shifts every part's value and hue a little (±7% and ±3%, repeatable from its name), so a wall of one material reads as many blocks (`TINT_*`);
- leans every baked normal 1.6 times further from flat (`NORMAL_GAIN`);
- writes each face's surface classes into the glTF.

Re-baking every building (about 4 minutes for a main building on four cores) is the second pull request; the units follow with the animation engine, because both touch the model pack.

## Built so far

Detail textures, the vertex tags, the shader layering, the renderer upload and the baker are built. The re-bake of the buildings and units, and the detail on scenery (the Living ground thread, using the same `detail.pak` and the same bake method), follow.
