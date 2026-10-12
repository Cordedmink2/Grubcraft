//! IMA ADPCM: 4 bits per sample, mono. Effects and music stay compressed in
//! memory and are decoded one sample at a time while mixing.
//!
//! Two samples per byte, low nibble first. Decoding starts from a zero
//! predictor and step index, so a clip needs no header.

const STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

const INDEX_ADJUST: [i32; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];

/// The decoder's running state: the last sample and the current step size.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Decoder {
    predictor: i32,
    index: i32,
}

impl Decoder {
    pub fn new() -> Decoder {
        Decoder::default()
    }

    /// Next sample from a 4-bit code.
    pub fn decode(&mut self, nibble: u8) -> i16 {
        let step = STEPS[self.index as usize];
        let mut diff = step >> 3;
        if nibble & 4 != 0 {
            diff += step;
        }
        if nibble & 2 != 0 {
            diff += step >> 1;
        }
        if nibble & 1 != 0 {
            diff += step >> 2;
        }
        if nibble & 8 != 0 {
            diff = -diff;
        }
        self.predictor = (self.predictor + diff).clamp(i16::MIN as i32, i16::MAX as i32);
        self.index = (self.index + INDEX_ADJUST[(nibble & 7) as usize]).clamp(0, 88);
        self.predictor as i16
    }
}

/// Compress samples. An odd count is padded with one extra code.
pub fn encode(samples: &[i16]) -> Vec<u8> {
    let mut dec = Decoder::new();
    let mut out = Vec::with_capacity(samples.len().div_ceil(2));
    let mut pending: Option<u8> = None;
    for &s in samples {
        let step = STEPS[dec.index as usize];
        let mut diff = s as i32 - dec.predictor;
        let mut nibble = 0u8;
        if diff < 0 {
            nibble = 8;
            diff = -diff;
        }
        if diff >= step {
            nibble |= 4;
            diff -= step;
        }
        if diff >= step >> 1 {
            nibble |= 2;
            diff -= step >> 1;
        }
        if diff >= step >> 2 {
            nibble |= 1;
        }
        dec.decode(nibble);
        match pending.take() {
            None => pending = Some(nibble),
            Some(low) => out.push(low | (nibble << 4)),
        }
    }
    if let Some(low) = pending {
        out.push(low);
    }
    out
}

/// Expand a whole clip; used by tests and tools (the mixer decodes per sample).
pub fn decode_all(data: &[u8], samples: usize) -> Vec<i16> {
    let mut dec = Decoder::new();
    (0..samples)
        .map(|i| {
            let byte = data[i / 2];
            dec.decode(if i % 2 == 0 { byte & 15 } else { byte >> 4 })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize) -> Vec<i16> {
        (0..n)
            .map(|i| ((i as f32 * 0.07).sin() * 12000.0 + (i as f32 * 0.31).sin() * 3000.0) as i16)
            .collect()
    }

    #[test]
    fn round_trip_stays_close() {
        let src = tone(4000);
        let packed = encode(&src);
        assert_eq!(packed.len(), 2000);
        let back = decode_all(&packed, src.len());
        let signal: f64 = src.iter().map(|&s| (s as f64).powi(2)).sum();
        let noise: f64 = src
            .iter()
            .zip(&back)
            .map(|(&a, &b)| (a as f64 - b as f64).powi(2))
            .sum();
        let snr_db = 10.0 * (signal / noise).log10();
        assert!(snr_db > 25.0, "SNR only {snr_db:.1} dB");
    }

    #[test]
    fn odd_length_is_padded() {
        assert_eq!(encode(&tone(7)).len(), 4);
        assert!(encode(&[]).is_empty());
    }

    #[test]
    fn silence_stays_silent() {
        let back = decode_all(&encode(&[0; 100]), 100);
        assert!(back.iter().all(|&s| s.abs() <= 7));
    }
}
