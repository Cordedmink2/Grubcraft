//! A picture of a map, for authors: height shading, water, cliffs, cover and
//! the objects. Written as a plain PNG with no compression, so it needs no
//! image library.

use crate::def::Kind;
use crate::map::{Ground, Map};
use gc_sim::terrain::Cover;

fn object_colour(k: Kind) -> [u8; 3] {
    match k {
        Kind::Start => [255, 60, 60],
        Kind::GrubSeep => [255, 230, 60],
        Kind::MarrowVein => [250, 250, 250],
        Kind::Village | Kind::Town | Kind::Bazaar => [255, 150, 40],
        Kind::CreepCamp | Kind::OldMouth => [190, 40, 200],
        Kind::Shrine => [60, 230, 230],
        Kind::Heart => [255, 0, 120],
        Kind::Bloodseep => [160, 20, 20],
        Kind::FrontierSlot => [120, 120, 255],
    }
}

/// RGB pixels, `scale` per tile, row by row.
pub fn render(map: &Map, scale: i32) -> (u32, Vec<u8>) {
    let side = (map.tiles * scale) as usize;
    let max = i32::from(*map.heights.iter().max().unwrap_or(&1)).max(1);
    let mut px = vec![0u8; side * side * 3];
    for y in 0..side {
        for x in 0..side {
            let t = ((y as i32 / scale) * map.tiles + x as i32 / scale) as usize;
            let h = i32::from(map.heights[t]) * 255 / max;
            let mut c = match map.ground[t] {
                Ground::Blocked if map.heights[t] < map.water => [20, 40, 110],
                Ground::Blocked => [95, 95, 100],
                Ground::Ford => [90, 150, 210],
                g => {
                    let shade = 70 + h / 3;
                    let (r, gr, b) = if g == Ground::Steep {
                        (shade + 20, shade, shade - 30)
                    } else {
                        (shade - 20, shade + 40, shade - 40)
                    };
                    [
                        r.clamp(0, 255) as u8,
                        gr.clamp(0, 255) as u8,
                        b.clamp(0, 255) as u8,
                    ]
                }
            };
            match Cover::from_u8(map.cover[t]) {
                Cover::Wood => c = [c[0] / 3, c[1] / 2, c[2] / 3],
                Cover::Grass => c = [c[0] / 2 + 40, c[1] / 2 + 70, c[2] / 3],
                Cover::Open => {}
            }
            px[(y * side + x) * 3..][..3].copy_from_slice(&c);
        }
    }
    for o in &map.objects {
        let colour = object_colour(o.kind);
        let r = if o.kind == Kind::Start || o.kind == Kind::Heart {
            2 * scale
        } else {
            scale
        };
        let (cx, cy) = (o.at.0 * scale + scale / 2, o.at.1 * scale + scale / 2);
        for y in (cy - r).max(0)..(cy + r).min(side as i32) {
            for x in (cx - r).max(0)..(cx + r).min(side as i32) {
                px[(y as usize * side + x as usize) * 3..][..3].copy_from_slice(&colour);
            }
        }
    }
    (side as u32, px)
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]) {
    out.extend((body.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend(kind);
    out.extend(body);
    let crc = crc32(&out[start..]);
    out.extend(crc.to_be_bytes());
}

pub fn png(side: u32, rgb: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgb.len() + side as usize);
    for row in rgb.chunks(side as usize * 3) {
        raw.push(0);
        raw.extend(row);
    }
    // zlib with stored (uncompressed) blocks.
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65_535).collect();
    for (i, b) in blocks.iter().enumerate() {
        z.push(u8::from(i + 1 == blocks.len()));
        z.extend((b.len() as u16).to_le_bytes());
        z.extend((!(b.len() as u16)).to_le_bytes());
        z.extend(*b);
    }
    let (mut a, mut s) = (1u32, 0u32);
    for &b in &raw {
        a = (a + u32::from(b)) % 65_521;
        s = (s + a) % 65_521;
    }
    z.extend(((s << 16) | a).to_be_bytes());
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut head = Vec::new();
    head.extend(side.to_be_bytes());
    head.extend(side.to_be_bytes());
    head.extend([8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &head);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}
