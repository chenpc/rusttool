//! MD5, as specified in RFC 1321.
//!
//! Not safe for anything that needs to resist an attacker, and the `*sum` tools
//! that print it say so. It is here because `md5sum` has to agree with the system
//! tool byte for byte, which is a compatibility question rather than a security
//! one.

use crate::Hasher;

/// The per-round shift amounts, from the specification's table.
const SHIFTS: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, //
    5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, //
    4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, //
    6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

/// `floor(2^32 * abs(sin(i + 1)))` for each of the 64 rounds.
const SINE: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
    0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
    0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
    0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
    0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
    0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
    0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
    0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
    0xeb86d391,
];

/// One 64-byte block's worth of MD5, in place.
///
/// A free function rather than a method so that `finish` can pad and compress
/// without having to reach back into `self` — which is where the first version of
/// this went wrong.
fn compress(state: &mut [u32; 4], block: &[u8; 64]) {
    let mut words = [0u32; 16];
    for (index, word) in words.iter_mut().enumerate() {
        let at = index * 4;
        *word = u32::from_le_bytes([block[at], block[at + 1], block[at + 2], block[at + 3]]);
    }
    let [mut a, mut b, mut c, mut d] = *state;
    for round in 0..64 {
        let (mixed, which) = match round / 16 {
            0 => ((b & c) | (!b & d), round),
            1 => ((d & b) | (!d & c), (5 * round + 1) % 16),
            2 => (b ^ c ^ d, (3 * round + 5) % 16),
            _ => (c ^ (b | !d), (7 * round) % 16),
        };
        let previous_d = d;
        d = c;
        c = b;
        let sum = a
            .wrapping_add(mixed)
            .wrapping_add(SINE[round])
            .wrapping_add(words[which]);
        b = b.wrapping_add(sum.rotate_left(SHIFTS[round]));
        a = previous_d;
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
}

/// The padded final blocks for `state` and `bits`.
fn pad_and_compress(mut state: [u32; 4], partial: &[u8], partial_len: usize, bits: u64) -> [u8; 16] {
    let mut block = [0u8; 64];
    block[..partial_len].copy_from_slice(&partial[..partial_len]);
    block[partial_len] = 0x80;
    if partial_len + 1 + 8 > 64 {
        // The length does not fit alongside the one bit, so this block is all
        // padding and the length goes in the next one.
        compress(&mut state, &block);
        block = [0; 64];
    }
    block[56..].copy_from_slice(&bits.to_le_bytes());
    compress(&mut state, &block);
    let mut out = [0u8; 16];
    for (index, word) in state.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    out
}

#[derive(Clone, Debug)]
pub struct Md5 {
    state: [u32; 4],
    /// The tail of the input not yet part of a 64-byte block.
    partial: [u8; 64],
    partial_len: usize,
    /// How many bytes have been fed, as a bit count.
    bits: u64,
}

impl Default for Md5 {
    fn default() -> Self {
        Md5 {
            state: [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476],
            partial: [0; 64],
            partial_len: 0,
            bits: 0,
        }
    }
}

impl Hasher for Md5 {
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
        pad_and_compress(self.state, &self.partial, self.partial_len, self.bits).to_vec()
    }

    fn digest_len(&self) -> Option<usize> {
        Some(16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::to_hex;

    /// Generated with `cksum -a md5`, not written by hand.
    fn md5(bytes: &[u8]) -> String {
        to_hex(&Md5::default().digest(bytes))
    }

    #[test]
    fn the_rfc1321_test_suite() {
        let cases = [
            ("", "d41d8cd98f00b204e9800998ecf8427e"),
            ("a", "0cc175b9c0f1b6a831c399e269772661"),
            ("abc", "900150983cd24fb0d6963f7d28e17f72"),
            ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
            ("abcdefghijklmnopqrstuvwxyz", "c3fcd3d76192e4007dfb496cca67e13b"),
            (
                "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
                "d174ab98d277d9f5a5611c2c9f419d9f",
            ),
            (
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "57edf4a22be3c955ac49da2e2107b67a",
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(md5(input.as_bytes()), expected, "input {input:?}");
        }
    }

    #[test]
    fn the_padding_length_boundary_is_right() {
        // These two differ only in whether the bit count still fits in the block
        // that already carries the one bit, which is the classic place to be off by
        // one block. The expected values come from `cksum -a md5`.
        for length in [0usize, 1, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128] {
            let bytes = vec![b'a'; length];
            assert_eq!(md5(&bytes).len(), 32, "length {length}");
        }
        // 56 is the last length that fits and 64 is the first whole block, so
        // between them the padding has to grow a block. Every value here was read
        // off `cksum -a md5`.
        let expected = [
            (55usize, "ef1772b6dff9a122358552954ad0df65"),
            (56, "3b0c8ac703f828b04c6c197006d17218"),
            (57, "652b906d60af96844ebd21b674f35e93"),
            (63, "b06521f39153d618550606be297466d5"),
            (64, "014842d480b571495a4a0363793f7367"),
            (65, "c743a45e0d2e6a95cb859adae0248435"),
            (119, "8a7bd0732ed6a28ce75f6dabc90e1613"),
            (120, "5f61c0ccad4cac44c75ff505e1f1e537"),
            (127, "020406e1d05cdc2aa287641f7ae2cc39"),
            (128, "e510683b3f5ffe4093d021808bc6ff70"),
        ];
        for (length, hex) in expected {
            assert_eq!(md5(&vec![b'a'; length]), hex, "{length} bytes");
        }
    }

    #[test]
    fn the_input_can_be_cut_anywhere_without_changing_the_digest() {
        let body = b"the quick brown fox jumps over the lazy dog, repeatedly and in pieces";
        let whole = Md5::default().digest(body);
        for cut in 0..body.len() {
            let mut hasher = Md5::default();
            hasher.update(&body[..cut]);
            hasher.update(&body[cut..]);
            assert_eq!(hasher.finish(), whole, "cut at {cut}");
        }
    }

    #[test]
    fn a_long_input_still_works() {
        assert_eq!(md5(&vec![b'x'; 100_000]).len(), 32);
        assert_eq!(md5(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(Md5::default().digest_len(), Some(16));
    }
}