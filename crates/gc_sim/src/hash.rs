use xxhash_rust::xxh3::Xxh3;

/// Hashes simulation state in an explicit, endian-independent byte order.
///
/// Never hash a struct's memory directly: padding and layout are not part of
/// the game state and could differ between compilers.
pub struct StateHasher(Xxh3);

impl StateHasher {
    pub fn new() -> StateHasher {
        StateHasher(Xxh3::new())
    }

    #[inline]
    pub fn u8(&mut self, v: u8) {
        self.0.update(&[v]);
    }

    #[inline]
    pub fn u16(&mut self, v: u16) {
        self.0.update(&v.to_le_bytes());
    }

    #[inline]
    pub fn u32(&mut self, v: u32) {
        self.0.update(&v.to_le_bytes());
    }

    #[inline]
    pub fn i32(&mut self, v: i32) {
        self.0.update(&v.to_le_bytes());
    }

    #[inline]
    pub fn u64(&mut self, v: u64) {
        self.0.update(&v.to_le_bytes());
    }

    #[inline]
    pub(crate) fn bytes(&mut self, v: &[u8]) {
        self.0.update(v);
    }

    pub fn finish(&self) -> u64 {
        self.0.digest()
    }
}

impl Default for StateHasher {
    fn default() -> Self {
        Self::new()
    }
}
