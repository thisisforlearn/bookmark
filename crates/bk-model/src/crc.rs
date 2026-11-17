// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! CRC32C (Castagnoli) — implemented in-house so the container format has no
//! external dependency. Uses the standard reflected polynomial 0x82F63B78.

/// CRC32C lookup table (reflected, Castagnoli).
pub struct Crc32C([u32; 256]);

impl Crc32C {
    pub fn new() -> Self {
        let mut table = [0u32; 256];
        for i in 0..256u32 {
            let mut crc = i;
            for _ in 0..8 {
                if crc & 1 != 0 {
                    crc = (crc >> 1) ^ 0x82F63B78;
                } else {
                    crc >>= 1;
                }
            }
            table[i as usize] = crc;
        }
        Self(table)
    }

    /// Update a running CRC with `data`. `crc` is the previous value (0 for a fresh run).
    #[inline]
    pub fn update(&self, crc: u32, data: &[u8]) -> u32 {
        let mut crc = crc ^ 0xFFFF_FFFF;
        for &b in data {
            crc = (crc >> 8) ^ self.0[((crc ^ b as u32) & 0xFF) as usize];
        }
        crc ^ 0xFFFF_FFFF
    }

    /// One-shot CRC32C of `data`.
    #[inline]
    pub fn checksum(&self, data: &[u8]) -> u32 {
        self.update(0, data)
    }
}

impl Default for Crc32C {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vector() {
        // CRC32C of the empty string is 0.
        let c = Crc32C::new();
        assert_eq!(c.checksum(b""), 0);
        // "123456789" -> 0xE3069283 (standard CRC32C test vector).
        assert_eq!(c.checksum(b"123456789"), 0xE3069283);
    }

    #[test]
    fn streaming_matches_oneshot() {
        let c = Crc32C::new();
        let data = (0..1000u32).map(|i| (i & 0xFF) as u8).collect::<Vec<_>>();
        let one = c.checksum(&data);
        let mut two = 0u32;
        for chunk in data.chunks(37) {
            two = c.update(two, chunk);
        }
        assert_eq!(one, two);
    }
}
