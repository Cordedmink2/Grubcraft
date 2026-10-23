#!/usr/bin/env python3
"""Procedural, seamless detail textures for Grubcraft.

For each of 15 material classes writes two 256x256 RGBA8 PNGs into
assets-src/detail/:

  <class>_d.png  RGB = albedo modulation (128 = neutral), A = cavity (255 = open)
  <class>_n.png  R,G = tangent-space normal x,y (128 = flat, OpenGL convention:
                 +y points to the top of the image), B = roughness delta
                 (128 = neutral), A = height

Every map is built from periodic functions only (FFT-filtered noise, toroidal
Voronoi, modulo-indexed geometry, np.roll for derivatives), so it tiles in
both directions. Each class has its own fixed seed.

Usage:  python3 -I scripts/gen_detail.py [--no-sheet]
"""

import argparse
import os
import time

import numpy as np
from PIL import Image, ImageDraw, ImageFont

W = 256                      # texel size of one tile (one tile = 0.5 m)
SEED_BASE = 7000             # class i uses seed SEED_BASE + i unless overridden
# chitin: its 14x14 plate layout puts a plate edge on the wrap column for some
# seeds, which trips the seam heuristic (ratio 1.0-2.4 across seeds). 7015 gives
# the largest margin on all four maps.
SEED_OVERRIDE = {"chitin": 7015}
REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT_DIR = os.path.join(REPO, "assets-src", "detail")
SHEET_PATH = ("/tmp/claude-0/-home-user-grubcraft/"
              "c8cdf441-87e3-54ff-9352-f9cf50e1db8b/scratchpad/detail-sheet.png")

# Frequency grids in cycles per tile, and tile-coordinate grids in [0, 1).
_FREQ = np.fft.fftfreq(W) * W
FX, FY = np.meshgrid(_FREQ, _FREQ)          # FX varies along columns (u)
_COORD = np.arange(W) / W
U, V = np.meshgrid(_COORD, _COORD)          # U = x (u), V = y (v), indexed [row, col]


# --------------------------------------------------------------------------
# Periodic building blocks
# --------------------------------------------------------------------------

def sstep(e0, e1, x):
    """Smooth Hermite step from 0 at e0 to 1 at e1."""
    t = np.clip((x - e0) / (e1 - e0), 0.0, 1.0)
    return t * t * (3.0 - 2.0 * t)


def pdiff(a, c):
    """Signed toroidal difference a - c in [-0.5, 0.5)."""
    return (a - c + 0.5) % 1.0 - 0.5


def noise(rng, f0, bw=0.7, su=1.0, sv=1.0):
    """Band-limited periodic noise, zero mean, unit std.

    f0 is the centre frequency in cycles per tile, bw the band width in
    octaves. su > 1 penalises frequencies along u, which stretches features
    along u (su = 16 gives grain-like streaks).
    """
    r = np.hypot(FX * su, FY * sv)
    lr = np.log2(np.maximum(r, 1e-3))
    amp = np.exp(-0.5 * ((lr - np.log2(f0)) / bw) ** 2)
    amp[0, 0] = 0.0
    n = np.fft.ifft2(np.fft.fft2(rng.standard_normal((W, W))) * amp).real
    n -= n.mean()
    return n / (n.std() + 1e-12)


def voronoi(rng, nx, ny, jitter=0.9):
    """Toroidal jittered-grid Voronoi.

    Returns (F1, F2, idx, px, py): distance to the nearest and second
    nearest feature point (tile units), index of the nearest point, and
    the point coordinates.
    """
    cx, cy = np.meshgrid(np.arange(nx), np.arange(ny))
    px = ((cx + 0.5 + jitter * (rng.random((ny, nx)) - 0.5)) / nx).ravel()
    py = ((cy + 0.5 + jitter * (rng.random((ny, nx)) - 0.5)) / ny).ravel()
    f1 = np.full((W, W), np.inf)
    f2 = np.full((W, W), np.inf)
    idx = np.zeros((W, W), dtype=np.int64)
    for k in range(px.size):
        dx = pdiff(U, px[k])
        dy = pdiff(V, py[k])
        d = np.sqrt(dx * dx + dy * dy)
        new = d < f1
        f2 = np.where(new, f1, np.minimum(f2, d))
        idx = np.where(new, k, idx)
        f1 = np.where(new, d, f1)
    return f1, f2, idx, px, py


def build(spec):
    """Turn a class spec into (albedo RGBA, normal RGBA) uint8 arrays.

    spec keys:
      tone      albedo offset as a fraction (x128 grey level), shape (W, W)
      amp       albedo scale in grey levels (default 128)
      tint_amt  optional (W, W) weight for a per-channel tint
      tint      optional (3,) per-channel offset multiplied by tint_amt
      cav       cavity amount 0..1 (1 = deepest crevice)
      cav_depth grey levels removed at full cavity (default 100, keeps A >= 155)
      h         height field (any units)
      rough     roughness delta, roughly -1..1 (positive = rougher)
      slope     target tangent of the 98th-percentile edge slope
    """
    # Albedo modulation.
    amp = spec.get("amp", 128.0)
    rgb = 128.0 + amp * spec["tone"][..., None] + np.zeros((W, W, 3))
    if spec.get("tint_amt") is not None:
        rgb = rgb + spec["tint_amt"][..., None] * np.asarray(spec["tint"], float)
    rgb = np.clip(np.round(rgb), 60, 200)

    # Cavity in alpha: 255 open, never below 150.
    cav = np.clip(spec["cav"], 0.0, 1.0)
    a_d = np.clip(np.round(255.0 - spec.get("cav_depth", 100.0) * cav), 150, 255)
    albedo = np.dstack([rgb, a_d]).astype(np.uint8)

    # Normal from height, periodic central differences.
    h = spec["h"]
    gx = 0.5 * (np.roll(h, -1, axis=1) - np.roll(h, 1, axis=1))   # d/dcol (+x)
    gy = 0.5 * (np.roll(h, -1, axis=0) - np.roll(h, 1, axis=0))   # d/drow (down)
    p98 = np.percentile(np.hypot(gx, gy), 98) + 1e-12
    s = spec["slope"] / p98
    # Up in the image is -row, so n_y = +d/drow and n_x = -d/dcol.
    nx = -gx * s
    ny = gy * s
    inv = 1.0 / np.sqrt(nx * nx + ny * ny + 1.0)
    nx = nx * inv
    ny = ny * inv
    r_n = np.clip(np.round(128.0 + 127.0 * nx), 0, 255)
    g_n = np.clip(np.round(128.0 + 127.0 * ny), 0, 255)
    b_n = np.clip(np.round(128.0 + 127.0 * np.clip(spec["rough"], -1.0, 1.0)), 0, 255)
    lo, hi = np.percentile(h, [0.5, 99.5])
    a_n = np.clip((h - lo) / (hi - lo + 1e-12), 0.0, 1.0) * 255.0
    normal = np.dstack([r_n, g_n, b_n, np.round(a_n)]).astype(np.uint8)
    return albedo, normal


def check_seamless(label, arr):
    """Edge seam must differ no more than 1.5x the typical neighbour difference."""
    a = arr.astype(np.int16)
    for axis, what in ((1, "columns"), (0, "rows")):
        seam = np.abs(np.take(a, 0, axis=axis) - np.take(a, -1, axis=axis)).mean()
        inner = np.abs(np.diff(a, axis=axis)).mean()
        assert seam <= 1.5 * inner + 1e-6, (
            f"{label}: seam across {what} not seamless "
            f"(edge diff {seam:.3f}, neighbour diff {inner:.3f})")


# --------------------------------------------------------------------------
# The 15 classes. Each takes a seeded rng and returns a spec dict.
# --------------------------------------------------------------------------

def cls_stone(rng):
    # Two courses of two blocks each, with per-course stagger and wobbly joints.
    vs = (V - 0.25) % 1.0                        # phase: tile edge falls mid-course
    course = np.floor(vs * 2).astype(int) % 2
    vin = (vs * 2) % 1.0
    off = rng.uniform(0.0, 0.5, 2)[course]
    wob = 0.012 * noise(rng, 5, 0.8)
    ur = (U - off) % 1.0
    blk = course * 2 + np.floor(ur * 2).astype(int)
    m = (ur % 0.5 + wob) % 0.5
    dx = np.minimum(m, 0.5 - m)
    dy = np.minimum(vin, 1.0 - vin) * 0.5
    dj = np.minimum(dx, dy)
    joint = 1.0 - sstep(0.005, 0.013, dj)
    bevel = 1.0 - sstep(0.013, 0.03, dj)
    btone = rng.uniform(-1, 1, 4) * 0.12
    bh = rng.uniform(-1, 1, 4) * 0.06
    face = noise(rng, 18, 0.8, su=3.0)          # chisel strokes
    fine = noise(rng, 70, 0.5)
    pits = sstep(1.7, 2.4, fine)
    return dict(
        tone=btone[blk] + 0.05 * face - 0.25 * joint - 0.10 * pits,
        cav=0.9 * joint + 0.5 * pits,
        h=0.3 * face + bh[blk] + 0.15 * bevel - 0.6 * joint - 0.5 * pits,
        rough=0.1 * face + 0.4 * joint + 0.3 * pits,
        slope=0.7)


def cls_brick(rng):
    # Running bond: 6 courses, 3 bricks per tile, alternate courses offset by half a brick.
    vs = (V - 1.0 / 12.0) % 1.0                  # phase: tile edge falls mid-course
    row = np.floor(vs * 6).astype(int)
    vin = (vs * 6) % 1.0
    off = (row % 2) / 6.0 + 1.0 / 12.0          # shift so the u seam falls mid-brick
    uo = (U - off) % 1.0
    bid = row * 3 + np.floor(uo * 3).astype(int)
    ut = ((uo * 3) % 1.0) / 3.0
    wob = 0.003 * noise(rng, 6, 0.8)
    ut = (ut + wob) % (1.0 / 3.0)
    dx = np.minimum(ut, 1.0 / 3.0 - ut)
    dy = np.minimum(vin, 1.0 - vin) / 6.0
    dj = np.minimum(dx, dy)
    mortar = 1.0 - sstep(0.007, 0.013, dj)
    bev = 1.0 - sstep(0.013, 0.03, dj)
    btone = rng.uniform(-1, 1, 18) * 0.10
    face = noise(rng, 30, 0.9)
    chips = sstep(2.0, 2.6, noise(rng, 50, 0.5)) * (1.0 - mortar)
    return dict(
        tone=btone[bid] * (1.0 - mortar) + 0.20 * mortar + 0.04 * face - 0.10 * chips,
        cav=0.75 * mortar + 0.5 * chips,
        h=-0.6 * mortar + 0.12 * bev * (1.0 - mortar) + 0.05 * face - 0.25 * chips,
        rough=0.15 * face + 0.35 * mortar + 0.15 * chips,
        slope=0.7)


def cls_wood(rng):
    # Long grain streaks along u, two rare knots with rings, fine pores.
    streak = 0.6 * noise(rng, 24, 1.2, su=16) + 0.4 * noise(rng, 6, 0.8, su=10)
    streak /= streak.std()
    ring = np.zeros((W, W))
    knot = np.zeros((W, W))
    for kx, ky in rng.random((2, 2)):
        du = pdiff(U, kx)
        dv = pdiff(V, ky)
        d = np.hypot(du * 0.35, dv)            # knots are elongated along the grain
        ring += np.sin(2 * np.pi * (d * 9 + 0.08 * streak)) * np.exp(-(d / 0.09) ** 2)
        knot += np.exp(-(d / 0.025) ** 2)
    pores = sstep(2.2, 2.8, noise(rng, 110, 0.5, su=4.0))
    return dict(
        tone=0.07 * streak + 0.04 * ring - 0.08 * knot - 0.06 * pores,
        cav=0.6 * pores + 0.3 * knot,
        h=0.3 * streak + 0.2 * ring - 0.4 * pores + 0.3 * knot,
        rough=0.15 * streak + 0.2 * pores,
        slope=0.3)


def cls_planks(rng):
    # Five horizontal planks, thin dark gaps, a butt joint per plank, two nails.
    NP = 5
    vs = (V - 0.1) % 1.0                         # phase: tile edge falls mid-plank
    prow = np.floor(vs * NP).astype(int)
    vin = (vs * NP) % 1.0
    gap = 1.0 - sstep(0.004, 0.009, np.minimum(vin, 1.0 - vin) / NP)
    ptone = rng.uniform(-0.07, 0.07, NP)
    butt = rng.uniform(0.0, 1.0, NP)
    ud = (U - butt[prow]) % 1.0
    butt_j = 1.0 - sstep(0.004, 0.009, np.minimum(ud, 1.0 - ud))
    streak = 0.6 * noise(rng, 20, 1.0, su=16) + 0.4 * noise(rng, 8, 0.8, su=10)
    nails = np.zeros((W, W))
    for _ in range(2):
        nv = (rng.integers(NP) + 0.5) / NP
        nu = rng.uniform()
        nails = np.maximum(nails, np.exp(-(np.hypot(pdiff(U, nu), pdiff(V, nv)) / 0.012) ** 2))
    return dict(
        tone=ptone[prow] + 0.06 * streak - 0.25 * gap - 0.12 * butt_j,
        cav=0.85 * gap + 0.5 * butt_j,
        h=0.15 * streak - 0.8 * gap - 0.5 * butt_j + 0.4 * nails,
        rough=0.2 * streak + 0.5 * gap + 0.1 * nails,
        slope=0.5)


def cls_plaster(rng):
    # Soft mottling, trowel arcs, a few faint hairline cracks (Voronoi edges).
    mott = noise(rng, 6, 0.9)
    trowel = noise(rng, 18, 0.5, su=2.5, sv=0.6)
    fine = noise(rng, 45, 0.7)
    f1, f2, _, _, _ = voronoi(rng, 7, 7, 0.9)
    crack = (1.0 - sstep(0.002, 0.006, f2 - f1)) * sstep(0.3, 0.7, noise(rng, 3, 0.6))
    return dict(
        tone=0.035 * mott + 0.02 * trowel + 0.01 * fine - 0.07 * crack,
        cav=0.35 * crack,
        h=0.25 * trowel + 0.2 * mott + 0.03 * fine - 0.25 * crack,
        rough=0.2 + 0.1 * fine,
        slope=0.2)


def cls_tile(rng):
    # Roof tiles: 8 staggered rows, 4 per row, curved (ramped) profile,
    # raised lower lip and a shadow line under each row's lower edge.
    vs = (V - 1.0 / 16.0) % 1.0                  # phase: tile edge falls mid-row
    row = np.floor(vs * 8).astype(int) % 8
    vin = (vs * 8) % 1.0
    uo = (U - 1.0 / 16.0 - 0.125 * (row % 2)) % 1.0   # shift so the u seam falls mid-tile
    col = np.floor(uo * 4).astype(int)
    s = (uo * 4) % 1.0                              # position across one tile
    tid = row * 4 + col
    tt = rng.uniform(-1, 1, 32)
    h_across = 0.5 - 0.5 * np.cos(np.pi * s)
    gside = 1.0 - sstep(0.01, 0.025, np.minimum(s, 1.0 - s) * 0.25)
    lip = np.exp(-((vin - 0.96) / 0.03) ** 2)
    sh = (1.0 - sstep(0.0, 0.08, vin)) ** 1.5
    mott = noise(rng, 12, 0.8)
    return dict(
        tone=tt[tid] * 0.05 + 0.03 * mott - 0.20 * sh - 0.12 * gside,
        cav=0.9 * sh + 0.6 * gside,
        h=0.7 * h_across + 0.2 * vin + 0.25 * lip - 0.5 * sh - 0.4 * gside,
        rough=0.1 + 0.2 * mott + 0.4 * gside,
        slope=0.6)


def cls_cloth(rng):
    # Plain weave, 48 threads each way, per-thread tone jitter, fine fuzz.
    a = U * 48.0
    b = V * 48.0
    i = np.floor(a).astype(int) % 48
    j = np.floor(b).astype(int) % 48
    pv = np.sin(np.pi * (a % 1.0))                  # warp thread profile
    pw = np.sin(np.pi * (b % 1.0))                  # weft thread profile
    wo = ((i + j) % 2) == 0                         # warp over at this crossing
    h = pv * np.where(wo, 1.0, 0.45) + pw * np.where(wo, 0.45, 1.0)
    wt = rng.uniform(-0.06, 0.06, 48)
    ft = rng.uniform(-0.06, 0.06, 48)
    thread_tone = np.where(wo, wt[i], ft[j])
    fuzz = noise(rng, 90, 0.5)
    valley = 1.0 - np.maximum(pv, pw)
    return dict(
        tone=thread_tone + 0.03 * np.where(wo, pv, pw) + 0.02 * fuzz,
        cav=0.8 * valley,
        h=h + 0.03 * fuzz,
        rough=0.25 + 0.2 * fuzz,
        slope=0.8)


def cls_leather(rng):
    # Pebbled grain from a 40x40 Voronoi, soft creases.
    f1, f2, idx, _, _ = voronoi(rng, 40, 40, 0.95)
    cv = rng.uniform(-1, 1, 1600)[idx]
    dome = f2 - f1                                  # 0 on cell borders, peak in the middle
    pebble = np.clip(dome / 0.012, 0.0, 1.0)
    seam = 1.0 - sstep(0.001, 0.004, dome)
    crease = np.exp(-(noise(rng, 5, 0.5) / 0.15) ** 2)
    return dict(
        tone=0.035 * cv + 0.04 * pebble,
        cav=0.7 * seam + 0.4 * crease,
        h=0.6 * pebble - 0.8 * crease,
        rough=0.1 + 0.2 * cv + 0.2 * seam,
        slope=0.45)


def cls_iron(rng):
    # Dark worked metal: brushed streaks along u, fine pits, a few scratches.
    streak = noise(rng, 40, 1.0, su=16)
    low = noise(rng, 4, 0.8)
    pits = sstep(1.9, 2.6, noise(rng, 90, 0.5))
    scratch = np.exp(-(noise(rng, 6, 0.6, su=8) / 0.12) ** 2) * sstep(0.6, 1.2, noise(rng, 4, 0.8))
    return dict(
        tone=-0.12 + 0.05 * streak + 0.04 * low - 0.15 * pits + 0.15 * scratch,
        cav=0.6 * pits + 0.3 * scratch,
        h=0.08 * streak + 0.06 * low - 0.5 * pits - 0.35 * scratch,
        rough=0.05 - 0.25 * streak + 0.3 * pits,
        slope=0.4)


def cls_bronze(rng):
    # Soft patina blotches (green-blue tint), pitting, low roughness variation.
    blot = noise(rng, 4, 0.8)
    blot2 = noise(rng, 10, 0.8)
    pits = sstep(1.9, 2.6, noise(rng, 80, 0.5))
    return dict(
        tone=0.04 * blot + 0.02 * blot2 - 0.12 * pits,
        tint_amt=sstep(0.2, 1.0, blot),
        tint=(-3.0, 5.0, 6.0),
        cav=0.7 * pits,
        h=0.12 * blot2 + 0.05 * blot - 0.3 * pits,
        rough=0.08 * blot2 + 0.05 * pits,
        slope=0.35)


def cls_bone(rng):
    # Smooth and warm: long fibre streaks along u, tiny pores, hairline cracks.
    streak = noise(rng, 30, 1.2, su=16)
    fine = noise(rng, 6, 0.8)
    pores = sstep(2.4, 3.0, noise(rng, 120, 0.4))
    f1, f2, _, _, _ = voronoi(rng, 5, 5, 0.9)
    cr = (1.0 - sstep(0.001, 0.0045, f2 - f1)) * sstep(0.2, 0.6, noise(rng, 3, 0.7))
    return dict(
        tone=0.03 * streak + 0.02 * fine - 0.05 * pores - 0.05 * cr,
        tint_amt=np.ones((W, W)),
        tint=(4.0, 1.0, -5.0),
        cav=0.5 * cr + 0.6 * pores,
        h=0.04 * streak + 0.04 * fine - 0.25 * cr - 0.15 * pores,
        rough=-0.2 + 0.1 * streak + 0.4 * cr + 0.1 * pores,
        slope=0.25)


def cls_chitin(rng):
    # Overlapping scales from a 14x14 Voronoi: domed plates, raised rims,
    # glossy centres, rough dark seams, per-plate tone jitter.
    f1, f2, idx, _, _ = voronoi(rng, 14, 14, 0.85)
    d = f2 - f1
    pj = rng.uniform(-1, 1, 196)[idx]
    dome = np.clip(d / 0.03, 0.0, 1.0)
    ridge = np.exp(-(d / 0.005) ** 2)
    seam = 1.0 - sstep(0.0, 0.01, d)
    return dict(
        tone=0.07 * pj + 0.04 * dome + 0.06 * ridge,
        cav=0.6 * (1.0 - sstep(0.002, 0.01, d)),
        h=0.6 * dome + 0.8 * ridge,
        rough=-0.35 * dome + 0.45 * seam,
        slope=0.6)


def cls_organic(rng):
    # Lumpy membrane with a branching vein network from zero-set ridges.
    lumps = 0.6 * noise(rng, 4, 0.9) + 0.4 * noise(rng, 12, 0.7)
    vein_n = 0.6 * noise(rng, 9, 0.6) + 0.4 * noise(rng, 16, 0.6)
    vein = np.exp(-(vein_n / 0.25) ** 2)
    return dict(
        tone=0.04 * lumps - 0.07 * vein,
        cav=0.4 * vein,
        h=0.5 * lumps + 0.15 * vein,
        rough=-0.55 + 0.1 * noise(rng, 10, 0.7),
        slope=0.4)


def cls_crystal(rng):
    # Planar facets: constant height per Voronoi cell plus a gentle tilt,
    # lighter facet edges, internal fracture lines, very glossy.
    NC = 7
    f1, f2, idx, px, py = voronoi(rng, NC, NC, 1.0)
    n = NC * NC
    cz = rng.uniform(-1, 1, n) * 0.5
    ax = rng.uniform(-1, 1, n) * 0.6
    ay = rng.uniform(-1, 1, n) * 0.6
    tf = rng.uniform(-1, 1, n)
    dx = pdiff(U, px[idx])
    dy = pdiff(V, py[idx])
    h = cz[idx] + ax[idx] * dx + ay[idx] * dy
    edge = 1.0 - sstep(0.002, 0.008, f2 - f1)
    frac = np.exp(-(noise(rng, 5, 0.6) / 0.12) ** 2) * sstep(0.2, 0.7, noise(rng, 3, 0.6))
    return dict(
        tone=0.08 * tf[idx] + 0.2 * edge - 0.12 * frac,
        cav=0.4 * frac + 0.2 * edge,
        h=h,
        rough=-0.7 + 0.08 * tf[idx] + 0.1 * frac,
        slope=0.8)


def cls_earth(rng):
    # Clumpy soil with small embedded pebble domes.
    clumps = 0.7 * noise(rng, 18, 0.9) + 0.3 * noise(rng, 50, 0.6)
    f1, _, idx, _, _ = voronoi(rng, 9, 9, 1.0)
    n = 81
    has = (rng.random(n) < 0.3)[idx]
    rad = rng.uniform(0.012, 0.022, n)[idx]
    ptone = rng.uniform(-0.12, 0.12, n)[idx]
    pebble = np.sqrt(np.clip(1.0 - (f1 / rad) ** 2, 0.0, 1.0)) * has
    contact = has * (1.0 - np.clip(pebble / 0.3, 0.0, 1.0))
    return dict(
        tone=0.08 * clumps + ptone * (pebble > 0),
        cav=0.5 * np.clip(-clumps, 0.0, 1.0) + 0.5 * contact,
        h=0.4 * clumps + 0.9 * pebble,
        rough=0.45 + 0.15 * clumps - 0.1 * pebble,
        slope=0.55)


CLASSES = [
    ("stone", cls_stone), ("brick", cls_brick), ("wood", cls_wood),
    ("planks", cls_planks), ("plaster", cls_plaster), ("tile", cls_tile),
    ("cloth", cls_cloth), ("leather", cls_leather), ("iron", cls_iron),
    ("bronze", cls_bronze), ("bone", cls_bone), ("chitin", cls_chitin),
    ("organic", cls_organic), ("crystal", cls_crystal), ("earth", cls_earth),
]


# --------------------------------------------------------------------------
# Contact sheet (optional, disabled by --no-sheet)
# --------------------------------------------------------------------------

def write_sheet(results, path):
    cell = 320          # size of each 2x2 block on the sheet
    pad = 14
    label_w = 170
    header = 24
    rows = len(results)
    sheet_w = label_w + pad + cell + pad + cell + pad
    sheet_h = pad + header + rows * (cell + pad)
    sheet = Image.new("RGB", (sheet_w, sheet_h), (40, 40, 44))
    draw = ImageDraw.Draw(sheet)
    try:
        font = ImageFont.load_default(size=16)
    except TypeError:
        font = ImageFont.load_default()

    x_alb = label_w + pad
    x_nrm = x_alb + cell + pad
    draw.text((x_alb, pad), "albedo (_d RGB), 2x2", fill=(220, 220, 220), font=font)
    draw.text((x_nrm, pad), "lit normal (_n), 2x2", fill=(220, 220, 220), font=font)

    light = np.array([-0.5, 0.5, 0.7])            # from upper left, towards viewer
    light /= np.linalg.norm(light)
    for k, (name, (albedo, normal)) in enumerate(results.items()):
        y = pad + header + k * (cell + pad)
        alb = np.tile(albedo[..., :3], (2, 2, 1))
        alb_img = Image.fromarray(alb, "RGB").resize((cell, cell), Image.LANCZOS)
        nx = (normal[..., 0].astype(float) - 128.0) / 127.0
        ny = (normal[..., 1].astype(float) - 128.0) / 127.0
        nz = np.sqrt(np.clip(1.0 - nx * nx - ny * ny, 0.0, 1.0))
        ndl = np.clip(nx * light[0] + ny * light[1] + nz * light[2], 0.0, 1.0)
        shade = np.clip(0.8 * (0.12 + 0.88 * ndl), 0.0, 1.0)
        lit = np.tile((shade * 255).astype(np.uint8), (2, 2))
        lit_img = Image.fromarray(lit, "L").resize((cell, cell), Image.LANCZOS).convert("RGB")
        sheet.paste(alb_img, (x_alb, y))
        sheet.paste(lit_img, (x_nrm, y))
        draw.text((pad, y + cell // 2 - 10), f"{k + 1:2d}  {name}", fill=(240, 240, 240), font=font)

    os.makedirs(os.path.dirname(path), exist_ok=True)
    sheet.save(path, optimize=True)
    print(f"contact sheet: {path}")


# --------------------------------------------------------------------------
# Driver
# --------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--no-sheet", action="store_true",
                        help="skip writing the contact sheet")
    args = parser.parse_args()

    t0 = time.time()
    os.makedirs(OUT_DIR, exist_ok=True)
    results = {}
    for cid, (name, fn) in enumerate(CLASSES, start=1):
        rng = np.random.default_rng(SEED_OVERRIDE.get(name, SEED_BASE + cid))
        albedo, normal = build(fn(rng))
        check_seamless(f"{name}_d", albedo)
        check_seamless(f"{name}_n", normal)

        d_path = os.path.join(OUT_DIR, f"{name}_d.png")
        n_path = os.path.join(OUT_DIR, f"{name}_n.png")
        Image.fromarray(albedo, "RGBA").save(d_path, optimize=True)
        Image.fromarray(normal, "RGBA").save(n_path, optimize=True)
        results[name] = (albedo, normal)
        print(f"{cid:2d} {name:8s} {os.path.getsize(d_path):7d} B  "
              f"{os.path.getsize(n_path):7d} B")

    if not args.no_sheet:
        write_sheet(results, SHEET_PATH)
    print(f"done in {time.time() - t0:.1f} s")


if __name__ == "__main__":
    main()
