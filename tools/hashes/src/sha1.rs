//! SHA-1, as specified in FIPS 180-4.

use crate::Hasher;

fn compress(state: &mut [u32; 5], block: &[u8; 64]) {
    let mut words = [0u32; 80];
    for (index, word) in words.iter_mut().take(16).enumerate() {
        let at = index * 4;
        *word = u32::from_be_bytes([block[at], block[at + 1], block[at + 2], block[at + 3]]);
    }
    for index in 16..80 {
        words[index] =
            (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                .rotate_left(1);
    }
    let [mut a, mut b, mut c, mut d, mut e] = *state;
    for (round, word) in words.iter().enumerate() {
        let (mixed, constant) = match round / 20 {
            0 => ((b & c) | (!b & d), 0x5a82_7999u32),
            1 => (b ^ c ^ d, 0x6ed9_eba1),
            2 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
            _ => (b ^ c ^ d, 0xca62_c1d6),
        };
        let temp = a
            .rotate_left(5)
            .wrapping_add(mixed)
            .wrapping_add(e)
            .wrapping_add(constant)
            .wrapping_add(*word);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temp;
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
}

fn pad_and_compress(
    mut state: [u32; 5],
    partial: &[u8],
    partial_len: usize,
    bits: u64,
) -> Vec<u8> {
    let mut block = [0u8; 64];
    block[..partial_len].copy_from_slice(&partial[..partial_len]);
    block[partial_len] = 0x80;
    if partial_len + 1 + 8 > 64 {
        compress(&mut state, &block);
        block = [0; 64];
    }
    block[56..].copy_from_slice(&bits.to_be_bytes());
    compress(&mut state, &block);
    let mut out = Vec::with_capacity(20);
    for word in state {
        out.extend_from_slice(&word.to_be_bytes());
    }
    out
}

#[derive(Clone, Debug)]
pub struct Sha1 {
    state: [u32; 5],
    partial: [u8; 64],
    partial_len: usize,
    bits: u64,
}

impl Default for Sha1 {
    fn default() -> Self {
        Sha1 {
            state: [
                0x6745_2301,
                0xefcd_ab89,
                0x98ba_dcfe,
                0x1032_5476,
                0xc3d2_e1f0,
            ],
            partial: [0; 64],
            partial_len: 0,
            bits: 0,
        }
    }
}

impl Hasher for Sha1 {
    fn update(&mut self, mut bytes: &[u8]) {
        self.bits = self.bits.wrapping_add((bytes.len() as u64) * 8);
        if self.partial_len > 0 {
            let take = (64 - self.partial_len).min(bytes.len());
            self.partial[self.partial_len..self.partial_len + take].copy_from_slice(&bytes[..take]);
            self.partial_len += take;
            bytes = &bytes[take..];
            if self.partial_len == 64 {
                let block = self.partial;
                compress(&mut self.state, &block);
                self.partial_len = 0;
            }
        }
        while bytes.len() >= 64 {
            let mut block = [0u8; 64];
            block.copy_from_slice(&bytes[..64]);
            compress(&mut self.state, &block);
            bytes = &bytes[64..];
        }
        if !bytes.is_empty() {
            self.partial[..bytes.len()].copy_from_slice(bytes);
            self.partial_len = bytes.len();
        }
    }

    fn finish(&mut self) -> Vec<u8> {
        pad_and_compress(self.state, &self.partial, self.partial_len, self.bits)
    }

    fn digest_len(&self) -> Option<usize> {
        Some(20)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::to_hex;

    #[test]
    fn the_fips180_examples() {
        assert_eq!(to_hex(&Sha1::default().digest(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(to_hex(&Sha1::default().digest(b"abc")), "a9993e364706816aba3e25717850c26c9cd0d89d");
        assert_eq!(
            to_hex(&Sha1::default().digest(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
    }

    #[test]
    fn a_million_a_is_the_well_known_answer() {
        // The standard long-input vector: it exercises the block counter far enough
        // to show the length is being counted in bits and not bytes.
        let mut hasher = Sha1::default();
        for _ in 0..1000 {
            hasher.update(&[b'a'; 1000]);
        }
        assert_eq!(to_hex(&hasher.finish()), "34aa973cd4c4daa4f61eeb2bdbad27316534016f");
    }

    #[test]
    fn the_padding_boundary() {
        // 55 bytes is the last length that fits its count, 56 forces another block.
        // 55 is the last length whose bit count fits in the block that already has
        // the one bit; 56 needs another. Every value read off `cksum -a sha1`.
        let expected = [
            (55usize, "c1c8bbdc22796e28c0e15163d20899b65621d65a"),
            (56, "c2db330f6083854c99d4b5bfb6e8f29f201be699"),
            (64, "0098ba824b5c16427bd7a1122a5a442a25ec644d"),
        ];
        for (length, hex) in expected {
            assert_eq!(to_hex(&Sha1::default().digest(&vec![b'a'; length])), hex, "{length}");
        }
    }

    #[test]
    fn the_input_can_be_cut_anywhere() {
        let body = b"a body of text for the sha1 chunking test, of a reasonable length";
        let whole = Sha1::default().digest(body);
        for cut in 0..body.len() {
            let mut hasher = Sha1::default();
            hasher.update(&body[..cut]);
            hasher.update(&body[cut..]);
            assert_eq!(hasher.finish(), whole, "cut at {cut}");
        }
        assert_eq!(Sha1::default().digest_len(), Some(20));
    }
}