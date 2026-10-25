"""Rig a static mesh as a non-biped unit and export a GLB gc_artbake can bake.

Meshy's auto-rig only handles bipeds (docs/39, research 01). Quadrupeds, riders
on mounts, flyers, crawlers and siege engines are rigged here instead, from a
rig *template*: a fixed set of bones and the standard clips. Every unit of one
template gets the same joint names, parents and rest pose, so units share one
skeleton and one clip set in the animation pack (docs/33), and a base mesh can
be recoloured into variants without a second rig (`--tint`).

    blender -b --python tools/blender_rig/rig_nonbiped.py -- \
        --in mesh.glb --out unit.glb --template quadruped [--tint 0.8,0.5,0.3]

Templates: quadruped (4 legs), crawler (6 legs, tripod gait), flyer (body and
two wings), engine (a hull that rolls and a swinging arm). Clips written, named
as in the `clips` map of assets-src: Idle, Walk, Run, Attack, Attack2, Hit, Die,
Cheer (a flyer's Walk is a slow hover). The mesh is rigidly skinned: each
vertex follows the nearest bone's region by position (no weight painting), good
enough for stand-ins and for Meshy bodies that are one lump.

The mesh should face +Z and stand on the ground at the origin (what the baker
expects); run with `--check-only` to print the bone and clip list without
writing. Run under Blender 4.0 or later (apt `blender`).
"""

import argparse
import math
import sys

import bpy
from mathutils import Vector

FPS = 24

# bone: (parent, head, tail) as shares of the mesh's box: x -1..1 across,
# y 0..1 up, z -1..1 along (+z is the front).
TEMPLATES = {
    "quadruped": {
        "root": (None, (0, 0.45, 0), (0, 0.5, 0)),
        "body": ("root", (0, 0.5, -0.3), (0, 0.5, 0.5)),
        "head": ("body", (0, 0.6, 0.5), (0, 0.7, 1.0)),
        "leg_fl": ("body", (0.5, 0.5, 0.5), (0.5, 0.0, 0.5)),
        "leg_fr": ("body", (-0.5, 0.5, 0.5), (-0.5, 0.0, 0.5)),
        "leg_bl": ("body", (0.5, 0.5, -0.5), (0.5, 0.0, -0.5)),
        "leg_br": ("body", (-0.5, 0.5, -0.5), (-0.5, 0.0, -0.5)),
    },
    "crawler": {
        "root": (None, (0, 0.4, 0), (0, 0.45, 0)),
        "body": ("root", (0, 0.45, -0.5), (0, 0.45, 0.4)),
        "head": ("body", (0, 0.5, 0.4), (0, 0.5, 1.0)),
        **{
            f"leg_{side}{row}": ("body", (x, 0.45, z), (x * 1.6, 0.0, z))
            for side, x in (("l", 0.4), ("r", -0.4))
            for row, z in (("1", 0.4), ("2", 0.0), ("3", -0.4))
        },
    },
    "flyer": {
        "root": (None, (0, 0.5, 0), (0, 0.55, 0)),
        "body": ("root", (0, 0.5, -0.5), (0, 0.5, 0.5)),
        "head": ("body", (0, 0.55, 0.5), (0, 0.6, 1.0)),
        "wing_l": ("body", (0.1, 0.6, 0.0), (1.0, 0.6, 0.0)),
        "wing_r": ("body", (-0.1, 0.6, 0.0), (-1.0, 0.6, 0.0)),
    },
    "engine": {
        "root": (None, (0, 0.3, 0), (0, 0.35, 0)),
        "hull": ("root", (0, 0.3, -0.5), (0, 0.3, 0.5)),
        "arm": ("hull", (0, 0.5, -0.2), (0, 1.0, 0.6)),
        "wheel_l": ("hull", (0.9, 0.2, 0.0), (1.0, 0.2, 0.0)),
        "wheel_r": ("hull", (-0.9, 0.2, 0.0), (-1.0, 0.2, 0.0)),
    },
}


def clear_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)


def import_mesh(path):
    bpy.ops.import_scene.gltf(filepath=path)
    meshes = [o for o in bpy.context.scene.objects if o.type == "MESH"]
    if not meshes:
        sys.exit("no mesh in " + path)
    for o in bpy.context.scene.objects:
        o.select_set(o.type == "MESH")
    bpy.context.view_layer.objects.active = meshes[0]
    if len(meshes) > 1:
        bpy.ops.object.join()
    mesh = bpy.context.view_layer.objects.active
    # Bake the import's transforms into the vertices and drop any parent.
    mesh.parent = None
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
    for o in list(bpy.context.scene.objects):
        if o is not mesh:
            bpy.data.objects.remove(o, do_unlink=True)
    return mesh


def mesh_box(mesh):
    pts = [mesh.matrix_world @ Vector(c) for c in mesh.bound_box]
    lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    return lo, hi


def to_world(box, share):
    """A template point (shares of the box) as a position in Blender's Z-up
    frame: glTF's +Y up and +Z forward arrive as Blender +Z up and -Y forward."""
    lo, hi = box
    mid = (lo + hi) / 2
    half = (hi - lo) / 2
    sx, sy, sz = share
    return Vector((mid.x + sx * half.x, mid.y - sz * half.y, lo.z + sy * (hi.z - lo.z)))


def build_armature(mesh, template):
    box = mesh_box(mesh)
    arm_data = bpy.data.armatures.new("Rig")
    arm = bpy.data.objects.new("Rig", arm_data)
    bpy.context.scene.collection.objects.link(arm)
    bpy.context.view_layer.objects.active = arm
    bpy.ops.object.mode_set(mode="EDIT")
    made = {}
    for name, (parent, head, tail) in TEMPLATES[template].items():
        b = arm_data.edit_bones.new(name)
        b.head, b.tail = to_world(box, head), to_world(box, tail)
        if (b.tail - b.head).length < 1e-4:
            b.tail = b.head + Vector((0, 0, 0.01))
        if parent:
            b.parent = made[parent]
        made[name] = b
    bpy.ops.object.mode_set(mode="OBJECT")
    return arm


def skin_rigid(mesh, arm):
    """Each vertex is weighted wholly to the nearest bone segment, except that
    the root and a limb's parent never take it: a leg moves only legs' own
    vertices, the body the rest (no weight painting, stand-in quality)."""
    for name in arm.data.bones.keys():
        mesh.vertex_groups.new(name=name)
    segs = {
        b.name: (arm.matrix_world @ b.head_local, arm.matrix_world @ b.tail_local)
        for b in arm.data.bones
        if b.name != "root"
    }

    def dist(p, a, c):
        ac = c - a
        t = max(0.0, min(1.0, (p - a).dot(ac) / max(ac.length_squared, 1e-9)))
        return (p - (a + ac * t)).length

    for v in mesh.data.vertices:
        p = mesh.matrix_world @ v.co
        best = min(segs, key=lambda n: dist(p, *segs[n]))
        mesh.vertex_groups[best].add([v.index], 1.0, "REPLACE")
    mesh.parent = arm
    mod = mesh.modifiers.new("Armature", "ARMATURE")
    mod.object = arm


def key(arm, bone, frame, loc=None, rot=None):
    pb = arm.pose.bones[bone]
    pb.rotation_mode = "XYZ"
    if loc is not None:
        pb.location = loc
        pb.keyframe_insert("location", frame=frame)
    if rot is not None:
        pb.rotation_euler = [math.radians(a) for a in rot]
        pb.keyframe_insert("rotation_euler", frame=frame)


def new_action(arm, name, length):
    act = bpy.data.actions.new(name)
    act.use_fake_user = True
    arm.animation_data_create().action = act
    for pb in arm.pose.bones:
        pb.location = (0, 0, 0)
        pb.rotation_euler = (0, 0, 0)
    return act, length


def cycle(arm, bone, length, axis, amp, phase=0.0):
    """Swing `bone` +-amp degrees about `axis` over `length` frames, looping."""
    for k in range(5):
        a = amp * math.sin(2 * math.pi * (k / 4 + phase))
        r = [0, 0, 0]
        r[axis] = a
        key(arm, bone, 1 + k * length // 4, rot=r)


def clips(arm, template):
    bones = set(arm.data.bones.keys())
    legs = sorted(b for b in bones if b.startswith("leg_"))
    wings = sorted(b for b in bones if b.startswith("wing_"))
    wheels = sorted(b for b in bones if b.startswith("wheel_"))
    head = "head" if "head" in bones else None
    main = "hull" if "hull" in bones else "body"
    out = []

    def make(name, length, fn):
        act, _ = new_action(arm, name, length)
        fn(length)
        act.frame_range = (1, length + 1)
        out.append(act)

    def idle(n):
        for k in range(3):
            key(arm, main, 1 + k * n // 2, loc=(0, 0, 0.012 * (k % 2)))
        if head:
            cycle(arm, head, n, 0, 3)

    def gait(speed, amp):
        def f(n):
            # Tripod or diagonal pairs: alternate legs by index parity.
            for i, leg in enumerate(legs):
                cycle(arm, leg, n, 0, amp, 0.5 if i % 2 else 0.0)
            for i, w in enumerate(wings):
                cycle(arm, w, n, 1, amp * 1.5, 0.5 if i % 2 else 0.0)
            for w in wheels:
                for k in range(5):
                    key(arm, w, 1 + k * n // 4, rot=(k * 90, 0, 0))
            cycle(arm, main, n, 1, 2 * speed)
        return f

    def hover(n):
        for i, w in enumerate(wings):
            cycle(arm, w, n, 1, 25, 0.5 if i % 2 else 0.0)
        for k in range(3):
            key(arm, main, 1 + k * n // 2, loc=(0, 0, 0.05 * (k % 2)))

    def strike(amp):
        def f(n):
            key(arm, main, 1, rot=(0, 0, 0))
            key(arm, main, n // 3, rot=(-amp, 0, 0))
            key(arm, main, n // 2, rot=(amp * 1.4, 0, 0))
            key(arm, main, n + 1, rot=(0, 0, 0))
            if head:
                key(arm, head, n // 2, rot=(amp, 0, 0))
                key(arm, head, n + 1, rot=(0, 0, 0))
            if "arm" in bones:
                key(arm, "arm", 1, rot=(0, 0, 0))
                key(arm, "arm", n // 2, rot=(-80, 0, 0))
                key(arm, "arm", n + 1, rot=(0, 0, 0))
        return f

    def hit(n):
        key(arm, main, 1, rot=(0, 0, 0))
        key(arm, main, n // 3, rot=(-12, 0, 0))
        key(arm, main, n + 1, rot=(0, 0, 0))

    def die(n):
        key(arm, main, 1, rot=(0, 0, 0), loc=(0, 0, 0))
        key(arm, main, n, rot=(0, 25, 0), loc=(0, 0, -0.1))
        key(arm, main, n + 1, rot=(0, 25, 0), loc=(0, 0, -0.1))
        for leg in legs:
            key(arm, leg, 1, rot=(0, 0, 0))
            key(arm, leg, n, rot=(0, 0, 40 if leg.endswith(("l", "l1", "l2", "l3")) else -40))
            key(arm, leg, n + 1, rot=(0, 0, 40 if leg.endswith(("l", "l1", "l2", "l3")) else -40))

    def cheer(n):
        for k in range(5):
            key(arm, main, 1 + k * n // 4, loc=(0, 0, 0.08 * (k % 2)))
        if head:
            cycle(arm, head, n, 0, -15)

    make("Idle", 48, idle)
    make("Walk", 24, hover if wings else gait(1, 25))
    make("Run", 14, gait(2, 35) if not wings else gait(2, 35))
    make("Attack", 20, strike(18))
    make("Attack2", 24, strike(28))
    make("Hit", 10, hit)
    make("Die", 30, die)
    make("Cheer", 36, cheer)
    return out


def tint_materials(mesh, rgb):
    """Recolour variants of one base mesh: multiply every material's base
    colour (no new texture, no new rig).

    Textured materials take their colour from an image, so changing the
    BSDF default does nothing.  Put a Multiply MixRGB (factor 1) between the
    colour source and Base Color; the glTF exporter writes a constant
    multiplier of a texture as the material's baseColorFactor.
    """
    for slot in mesh.material_slots:
        m = slot.material
        if not m or not m.use_nodes:
            continue
        nt = m.node_tree
        for n in list(nt.nodes):
            if n.type != "BSDF_PRINCIPLED":
                continue
            sock = n.inputs["Base Color"]
            mix = nt.nodes.new("ShaderNodeMix")
            mix.data_type = "RGBA"
            mix.blend_type = "MULTIPLY"
            mix.inputs[0].default_value = 1.0
            mix.inputs[7].default_value = (rgb[0], rgb[1], rgb[2], 1.0)
            if sock.links:
                src = sock.links[0].from_socket
                nt.links.new(src, mix.inputs[6])
            else:
                c = sock.default_value
                mix.inputs[6].default_value = (c[0], c[1], c[2], c[3])
            nt.links.new(mix.outputs[2], sock)


def main():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    ap = argparse.ArgumentParser()
    ap.add_argument("--in", dest="src", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--template", choices=sorted(TEMPLATES), required=True)
    ap.add_argument("--tint", help="r,g,b multipliers, e.g. 0.8,0.5,0.3")
    ap.add_argument("--check-only", action="store_true")
    a = ap.parse_args(argv)

    clear_scene()
    bpy.context.scene.render.fps = FPS
    mesh = import_mesh(a.src)
    if a.tint:
        tint_materials(mesh, [float(x) for x in a.tint.split(",")])
    arm = build_armature(mesh, a.template)
    skin_rigid(mesh, arm)
    acts = clips(arm, a.template)
    print("RIG template=%s bones=%s" % (a.template, ",".join(arm.data.bones.keys())))
    print("RIG clips=%s" % ",".join(x.name for x in acts))
    if a.check_only:
        return
    arm.animation_data.action = None
    bpy.ops.object.select_all(action="DESELECT")
    mesh.select_set(True)
    arm.select_set(True)
    bpy.ops.export_scene.gltf(
        filepath=a.out,
        export_format="GLB",
        use_selection=True,
        export_animations=True,
        export_animation_mode="ACTIONS",
        export_force_sampling=True,
        export_apply=False,
    )
    print("RIG wrote " + a.out)


main()
