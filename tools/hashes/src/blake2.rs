//! BLAKE2b, as specified in RFC 7693.
//!
//! The only thing unusual about it is the one structural difference from a Merkle–Damgård
//! hash: there is no padding. The last block of the message is flagged instead, which
//! means a hasher has to know when it is being fed the final block. That is why
//! [`Blake2b`] has an explicit [`Blake2b::finish`] rather than being able to answer at
//! any moment, and why `update` is never allowed to compress a block it does not know
//! to be the last one.
//!
//! The digest length is a parameter rather than fixed, and it is mixed into the
//! initial state — so a 256-bit BLAKE2b is not the first 32 bytes of a 512-bit one, and
//! `b2sum -l` needs this to be right for every width it offers.

use crate::Hasher;

/// The same initial values as SHA-512, which the specification reuses.
const IV: [u64; 8] = [
    0x6a09_e667_f3bc_c908, 0xbb67_ae85_84ca_a73b, 0x3c6e_f372_fe94_f82b, 0xa54f_f53a_5f1d_36f1,
    0x510e_527f_ade6_82d1, 0x9b05_688c_2b3e_6c1f, 0x1f83_d9ab_fb41_bd6b, 0x5be0_cd19_137e_2179,
];

/// The twelve message word permutations, one per round.
const SIGMA: [[usize; 16]; 12] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
];

/// One BLAKE2b hasher at a chosen digest width.
#[derive(Clone, Debug)]
pub struct Blake2b {
    state: [u64; 8],
    /// The tail of the input not yet part of a 128-byte block.
    partial: [u8; 128],
    partial_len: usize,
    /// How many bytes have been fed, counted as a 128-bit counter.
    counter: u128,
    out_len: usize,
}

impl Blake2b {
    /// A hasher whose digest is `bits / 8` bytes long.
    ///
    /// `bits` must be a multiple of 8 and at most 512, which is what `cksum -l`
    /// checks before it gets here.
    pub fn new(bits: usize) -> Blake2b {
        let out_len = bits / 8;
        let mut state = IV;
        // The parameter block: fanout 1, depth 1, no key, and the digest length in
        // the low byte. This is why the width changes the digest rather than
        // truncating one.
        state[0] ^= 0x0101_0000 ^ (out_len as u64);
        Blake2b {
            state,
            partial: [0; 128],
            partial_len: 0,
            counter: 0,
            out_len,
        }
    }

    /// The mix function, applied to the working vector and one message block.
    fn compress(&mut self, block: &[u8; 128], last: bool) {
        let mut m = [0u64; 16];
        for (index, word) in m.iter_mut().enumerate() {
            let at = index * 8;
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&block[at..at + 8]);
            *word = u64::from_le_bytes(bytes);
        }
        let mut v = [0u64; 16];
        v[..8].copy_from_slice(&self.state);
        v[8..].copy_from_slice(&IV);
        // The low half counts bytes so far and the high half the total, which is
        // what lets the implementation stream without knowing the length up front.
        v[12] ^= self.counter as u64;
        v[13] ^= (self.counter >> 64) as u64;
        if last {
            v[14] = !v[14];
        }
        for round in SIGMA {
            mix(&mut v, 0, 4, 8, 12, m[round[0]], m[round[1]]);
            mix(&mut v, 1, 5, 9, 13, m[round[2]], m[round[3]]);
            mix(&mut v, 2, 6, 10, 14, m[round[4]], m[round[5]]);
            mix(&mut v, 3, 7, 11, 15, m[round[6]], m[round[7]]);
            mix(&mut v, 0, 5, 10, 15, m[round[8]], m[round[9]]);
            mix(&mut v, 1, 6, 11, 12, m[round[10]], m[round[11]]);
            mix(&mut v, 2, 7, 8, 13, m[round[12]], m[round[13]]);
            // The eighth call continues the same sequence: indices 14 and 15.
            // Writing `round[0]` here instead looks harmless and is not, because an
            // all-zero message makes every index give the same word — which is
            // exactly what the first version of this did, and why every test on
            // zero-length input passed while every real input failed.
            mix(&mut v, 3, 4, 9, 14, m[round[14]], m[round[15]]);
        }
        for index in 0..8 {
            self.state[index] ^= v[index] ^ v[index + 8];
        }
    }
}

/// The quarter-round-ish `G` function, with BLAKE2b's four rotation constants.
fn mix(v: &mut [u64; 16], a: usize, b: usize, c: usize, d: usize, x: u64, y: u64) {
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(x);
    v[d] = (v[d] ^ v[a]).rotate_right(32);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(24);
    v[a] = v[a].wrapping_add(v[b]).wrapping_add(y);
    v[d] = (v[d] ^ v[a]).rotate_right(16);
    v[c] = v[c].wrapping_add(v[d]);
    v[b] = (v[b] ^ v[c]).rotate_right(63);
}

impl Hasher for Blake2b {
    fn update(&mut self, bytes: &[u8]) {
        // A block is compressed only once a byte *beyond* it has arrived, because
        // until then it might be the last one — and there is no padding to append, so
        // the flag is the only thing that marks it. Getting this wrong shows up at
        // exactly one length and nowhere else: an input of a whole number of blocks.
        for byte in bytes {
            if self.partial_len == 128 {
                let block = self.partial;
                self.compress(&block, false);
                self.partial_len = 0;
            }
            self.partial[self.partial_len] = *byte;
            self.partial_len += 1;
            self.counter += 1;
        }
    }

    fn finish(&mut self) -> Vec<u8> {
        let out_len = self.out_len;
        // Zero-fill the rest of the last block, then compress it flagged as final:
        // there is no padding, so the tail is short and the flag is what tells the
        // compression function not to wait for more.
        self.partial[self.partial_len..].fill(0);
        let block = self.partial;
        self.compress(&block, true);
        let mut out = Vec::with_capacity(out_len);
        for word in self.state {
            out.extend_from_slice(&word.to_le_bytes());
            if out.len() >= out_len {
                break;
            }
        }
        out.truncate(out_len);
        out
    }

    fn digest_len(&self) -> Option<usize> {
        Some(self.out_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::to_hex;

    #[test]
    fn the_rfc7693_vector_for_abc() {
        assert_eq!(
            to_hex(&Blake2b::new(512).digest(b"abc")),
            "ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923"
        );
    }

    #[test]
    fn the_empty_input_at_every_offered_width() {
        // The widths `cksum -l` accepts, on an empty input.
        let expected = [
            (8usize, "2e"),
            (16, "b1fe"),
            (32, "1271cf25"),
        ];
        for (bits, hex) in expected {
            assert_eq!(to_hex(&Blake2b::new(bits).digest(b"")), hex, "{bits} bits");
        }
        assert_eq!(Blake2b::new(512).digest(b"").len(), 64);
    }

    #[test]
    fn a_narrow_digest_is_not_a_prefix_of_a_wide_one() {
        // The digest length is mixed into the initial state, so this holds and has to:
        // if it did not, `b2sum -l 128` would be truncating rather than hashing.
        let wide = Blake2b::new(512).digest(b"some input");
        let narrow = Blake2b::new(128).digest(b"some input");
        assert_ne!(wide[..16], narrow[..16]);
    }

    #[test]
    fn the_input_can_be_cut_anywhere() {
        let body = b"a body of text for the blake2b chunking test, of a reasonable length";
        let whole = Blake2b::new(256).digest(body);
        for cut in 0..body.len() {
            let mut hasher = Blake2b::new(256);
            hasher.update(&body[..cut]);
            hasher.update(&body[cut..]);
            assert_eq!(hasher.finish(), whole, "cut at {cut}");
        }
    }

    #[test]
    fn a_whole_number_of_blocks_is_the_boundary() {
        // 128 bytes fills a block exactly, and that block is also the last one, so it
        // has to carry the flag. A hasher that compresses as soon as its buffer fills
        // is wrong at exactly these lengths and nowhere else, which is why they are
        // the only ones with values attached: every other length agrees either way.
        // Read off `cksum -a blake2b -l 256`.
        let expected = [
            (127usize, "59e2f1aba240f20aa591016f5ef429990bc9c2131dcd0d30f0ffd75ed18f317d"),
            (128, "ae2aa48507885c4c950fb809b2076f959cde9f8ea6da260d9a3587df33dac450"),
            (129, "2f64744a6de0d2c0b56e64cf6e29a5aaa255010d415d51c75ccc82f73dccd865"),
            (255, "177ec7b22a982dd81ec80e0f8fd488bb347952a0876fed488191b6dede62df81"),
            (256, "eae4d3a7627549b383179dc18049964f91a6fed14c9f3fb26705eda3eeda5558"),
        ];
        for (length, hex) in expected {
            let mut hasher = Blake2b::new(256);
            assert_eq!(to_hex(&hasher.digest(&vec![b'a'; length])), hex, "{length} bytes");
        }
        // The same inputs fed in pieces, which is the case a streaming reader hits.
        let body = vec![b'a'; 300];
        let mut whole = Blake2b::new(256);
        let reference = whole.digest(&body);
        for chunk in [1usize, 7, 64, 127, 128, 129, 256] {
            let mut hasher = Blake2b::new(256);
            for piece in body.chunks(chunk) {
                hasher.update(piece);
            }
            assert_eq!(hasher.finish(), reference, "in chunks of {chunk}");
        }
    }
}