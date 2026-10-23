#!/usr/bin/env python3
"""Bake the ground textures the terrain shader blends (assets/textures/terrain).

Usage: bake.py GRASS001_DIR GROUND054_DIR OUT_DIR
Each DIR is an unzipped ambientCG 1K JPG set (CC0); OUT_DIR is assets/textures/terrain.
Writes grass001_albedo.png, grass001_normal.png, ground054_albedo.png and ground054_normal.png,
512x512 8-bit RGB, resized with Pillow LANCZOS (normals: the OpenGL-convention map, renormalised).
"""
import sys
from pathlib import Path

import numpy as np
from PIL import Image

SIDE = 512


def bake(src: Path, name: str, out: Path) -> None:
    key = f"{name.capitalize()}"
    color = Image.open(src / f"{key}_1K-JPG_Color.jpg").convert("RGB")
    color.resize((SIDE, SIDE), Image.LANCZOS).save(out / f"{name}_albedo.png", optimize=True)
    n = Image.open(src / f"{key}_1K-JPG_NormalGL.jpg").convert("RGB")
    v = np.asarray(n.resize((SIDE, SIDE), Image.LANCZOS)).astype(np.float32) / 127.5 - 1.0
    v /= np.maximum(np.linalg.norm(v, axis=2, keepdims=True), 1e-4)
    Image.fromarray(np.clip((v * 0.5 + 0.5) * 255.0 + 0.5, 0, 255).astype(np.uint8)).save(
        out / f"{name}_normal.png", optimize=True
    )


if __name__ == "__main__":
    grass, ground, out = (Path(a) for a in sys.argv[1:4])
    bake(grass, "grass001", out)
    bake(ground, "ground054", out)
