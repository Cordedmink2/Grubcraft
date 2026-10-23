#!/usr/bin/env python3
"""Bake the ground and scenery detail textures the shaders blend (assets/textures/terrain).

Usage: bake.py OUT_DIR NAME=DIR [NAME=DIR ...]
Each DIR is an unzipped ambientCG 1K JPG set (CC0) and NAME its asset id in lower case
(grass001, ground054, bark001, bark014, moss002); OUT_DIR is assets/textures/terrain.
Writes NAME_albedo.png and NAME_normal.png: 512x512 8-bit RGB, resized with Pillow LANCZOS
(normals: the OpenGL-convention map, renormalised).
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
    out = Path(sys.argv[1])
    for pair in sys.argv[2:]:
        name, src = pair.split("=", 1)
        bake(Path(src), name, out)
