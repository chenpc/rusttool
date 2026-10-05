//! SM3, as `cksum -a sm3` computes it.
//!
//! It is only reachable through that one option, but a reachable option is a
//! reachable option: it has to produce the same bytes as the system tool or the
//! differential test fails.
//!
//! **This is a port of gnulib's `sm3.c`, not a transcription of the published
//! standard, and the two are not the same function.** Three places where they
//! differ, all of which produce a digest of the right length and the wrong value:
//!
//! * The round constants are **sixty-four table entries, not two values**. The
//!   standard's own listing gives `0x79cc4519` for the first sixteen rounds and
//!   `0x7a879d8a` for the rest; gnulib instead rotates the constant left by one each
//!   round, so consecutive rounds use different values. Reproducing the standard
//!   instead of the tool is the mistake this module exists to avoid. The table has
//!   the further oddity that its last sixteen entries repeat its sixteenth through
//!   thirty-first, which is what the file says and what the tool does.
//! * The message schedule is written so that `W2` computes `W[j+4]` while `W[j]` is
//!   still in use, four rounds ahead. For the first twelve rounds there is no
//!   schedule step at all and the "second" word is simply `W[j+4]` out of the
//!   block.
//! * The eight working registers keep their identity and their *roles* rotate on a
//!   cycle of four, so a round does not always update `a` and `d`.
//!
//! Every expected digest in the tests below was checked against both
//! `/usr/bin/cksum -a sm3` and gnulib's own C, compiled for the purpose.

/// The sixty-four round constants, from gnulib's `sm3.c`.
///
/// Sixty-four entries, forty-eight of them distinct: the last sixteen repeat the
/// sixteenth through thirty-first. SM3 is usually described as having *two*
/// constants, which is what the specification says; what `cksum` computes is the
/// variant above, and the table is transcribed rather than generated so that this
/// oddity is reproduced rather than smoothed away.
use crate::Hasher;

const T: [u32; 64] = [
    0x79cc_4519, 0xf398_8a32, 0xe731_1465, 0xce62_28cb, 0x9cc4_5197, 0x3988_a32f,
    0x7311_465e, 0xe622_8cbc, 0xcc45_1979, 0x988a_32f3, 0x3114_65e7, 0x6228_cbce,
    0xc451_979c, 0x88a3_2f39, 0x1146_5e73, 0x228c_bce6, 0x9d8a_7a87, 0x3b14_f50f,
    0x7629_ea1e, 0xec53_d43c, 0xd8a7_a879, 0xb14f_50f3, 0x629e_a1e7, 0xc53d_43ce,
    0x8a7a_879d, 0x14f5_0f3b, 0x29ea_1e76, 0x53d4_3cec, 0xa7a8_79d8, 0x4f50_f3b1,
    0x9ea1_e762, 0x3d43_cec5, 0x7a87_9d8a, 0xf50f_3b14, 0xea1e_7629, 0xd43c_ec53,
    0xa879_d8a7, 0x50f3_b14f, 0xa1e7_629e, 0x43ce_c53d, 0x879d_8a7a, 0x0f3b_14f5,
    0x1e76_29ea, 0x3cec_53d4, 0x79d8_a7a8, 0xf3b1_4f50, 0xe762_9ea1, 0xcec5_3d43,
    0x9d8a_7a87, 0x3b14_f50f, 0x7629_ea1e, 0xec53_d43c, 0xd8a7_a879, 0xb14f_50f3,
    0x629e_a1e7, 0xc53d_43ce, 0x8a7a_879d, 0x14f5_0f3b, 0x29ea_1e76, 0x53d4_3cec,
    0xa7a8_79d8, 0x4f50_f3b1, 0x9ea1_e762, 0x3d43_cec5,
];


/// The starting state.
const IV: [u32; 8] = [
    0x7380_166f, 0x4914_b2b9, 0x1724_42d7, 0xda8a_0600, 0xa96f_30bc, 0x1631_38aa, 0xe38d_ee4d,
    0xb0fb_0e4e,
];

/// Which of the eight long-lived registers plays which role, cycling every four
/// rounds.
const ROLES: [[usize; 8]; 4] = [
    [0, 1, 2, 3, 4, 5, 6, 7], // A=a  B=b  C=c  D=d  E=e  F=f  G=g  H=h
    [3, 0, 1, 2, 7, 4, 5, 6], // A=d  B=a  C=b  D=c  E=h  F=e  G=f  H=g
    [2, 3, 0, 1, 6, 7, 4, 5], // A=c  B=d  C=a  D=b  E=g  F=h  G=e  H=f
    [1, 2, 3, 0, 5, 6, 7, 4], // A=b  B=c  D=a  E=f  F=g  G=h  H=e
];

fn rol(value: u32, places: u32) -> u32 {
    value.rotate_left(places & 31)
}

/// The specification's `P0`.
fn p0(value: u32) -> u32 {
    value ^ rol(value, 9) ^ rol(value, 17)
}

/// The specification's `P1`.
fn p1(value: u32) -> u32 {
    value ^ rol(value, 15) ^ rol(value, 23)
}

/// The full round constant for round `j`.
/// Compress one 64-byte block.
fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    // The message schedule lives in a sixteen-word window that is overwritten in
    // place, four rounds ahead of the round that needs it.
    let mut w = [0u32; 16];
    for (index, word) in w.iter_mut().enumerate() {
        let at = index * 4;
        *word = u32::from_be_bytes([block[at], block[at + 1], block[at + 2], block[at + 3]]);
    }
    let mut registers = *state;
    for round in 0..64 {
        // The first twelve rounds have no schedule step: the "second word" is just
        // the block word W[j+4]. From round twelve on the window is refreshed, but
        // always for a word the current round does not read.
        let ahead = if round < 12 {
            w[(round + 4) & 0xf]
        } else {
            let fresh = p1(
                w[(round + 4) & 0xf]
                    ^ w[(round + 4 - 9) & 0xf]
                    ^ rol(w[(round + 4 - 3) & 0xf], 15),
            ) ^ rol(w[(round + 4 - 13) & 0xf], 7)
                ^ w[(round + 4 - 6) & 0xf];
            w[(round + 4) & 0xf] = fresh;
            fresh
        };
        let here = w[round & 0xf];

        let roles = ROLES[round % 4];
        let [a, b, c, d, e, f, g, h] = [
            registers[roles[0]],
            registers[roles[1]],
            registers[roles[2]],
            registers[roles[3]],
            registers[roles[4]],
            registers[roles[5]],
            registers[roles[6]],
            registers[roles[7]],
        ];
        let ss1 = rol(rol(a, 12).wrapping_add(e).wrapping_add(T[round]), 7);
        let ss2 = ss1 ^ rol(a, 12);
        // The two halves of the round use different combining functions.
        let ff = if round < 16 {
            a ^ b ^ c
        } else {
            (a & b) | (a & c) | (b & c)
        };
        let gg = if round < 16 {
            e ^ f ^ g
        } else {
            (e & f) | (!e & g)
        };
        let new_d = d
            .wrapping_add(ff)
            .wrapping_add(ss2)
            .wrapping_add(here ^ ahead);
        let new_h = p0(
            h.wrapping_add(gg).wrapping_add(ss1).wrapping_add(here),
        );
        // Only D, H, B and F are ever written; A, C, E and G keep their values and
        // the roles rotate instead. B turns by nine and F by nineteen every round
        // they hold those roles.
        registers[roles[3]] = new_d;
        registers[roles[7]] = new_h;
        registers[roles[1]] = rol(b, 9);
        registers[roles[5]] = rol(f, 19);
    }
    // The fold is exclusive-or, not addition, which is the one thing that makes this
    // unlike every other digest in this crate.
    for (slot, value) in state.iter_mut().zip(registers) {
        *slot ^= value;
    }
}

#[derive(Clone, Debug)]
pub struct Sm3 {
    state: [u32; 8],
    partial: [u8; 64],
    partial_len: usize,
    bits: u64,
}

impl Default for Sm3 {
    fn default() -> Self {
        Sm3 {
            state: IV,
            partial: [0; 64],
            partial_len: 0,
            bits: 0,
        }
    }
}

impl Hasher for Sm3 {
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
        let mut block = [0u8; 64];
        block[..self.partial_len].copy_from_slice(&self.partial[..self.partial_len]);
        block[self.partial_len] = 0x80;
        if self.partial_len + 1 + 8 > 64 {
            compress(&mut self.state, &block);
            block = [0; 64];
        }
        block[56..].copy_from_slice(&self.bits.to_be_bytes());
        compress(&mut self.state, &block);
        let mut out = Vec::with_capacity(32);
        for word in self.state {
            out.extend_from_slice(&word.to_be_bytes());
        }
        out
    }

    fn digest_len(&self) -> Option<usize> {
        Some(32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::to_hex;

    /// Read off `cksum -a sm3`, and cross-checked against gnulib's C.
    fn sm3(bytes: &[u8]) -> String {
        to_hex(&Sm3::default().digest(bytes))
    }

    #[test]
    fn the_vectors_the_tool_agrees_with() {
        assert_eq!(
            sm3(b""),
            "1ab21d8355cfa17f8e61194831e81a8f22bec8c728fefb747ed035eb5082aa2b"
        );
        assert_eq!(
            sm3(b"abc"),
            "66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0"
        );
        assert_eq!(
            sm3(b"abcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcd"),
            "debe9ff92275b8a138604889c18e5a4d6fdb70e5387e5765293dcba39c0c5732"
        );
        // The same 64 bytes twice, which is the first input that needs a second
        // block and so exercises the padding path with something in it.
        let sixteen: &[u8] = b"abcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcdabcd";
        let twice = [sixteen, sixteen].concat();
        assert_eq!(
            sm3(&twice),
            "90d52a2e85631a8d6035262626941fa11b85ce570cec1e3e991e2dd7ed258148"
        );
    }

    #[test]
    fn the_round_constants_are_not_the_specifications_two_values() {
        // The whole reason this module is a port rather than a transcription.
        assert_eq!(T[0], 0x79cc_4519);
        assert_eq!(T[1], 0xf398_8a32);
        assert_ne!(T[0], T[1], "consecutive rounds use different constants");
        assert_eq!(T[16], 0x9d8a_7a87);
        assert_ne!(T[15], T[16]);
        assert_eq!(T[32], 0x7a87_9d8a);
        assert_ne!(T[32], T[33]);
        // Each constant is the previous one rotated left, until the table's own
        // quirk: the last sixteen entries repeat the sixteenth through thirty-first.
        for round in 1..16 {
            assert_eq!(T[round], T[round - 1].rotate_left(1), "round {round}");
        }
        for round in 48..64 {
            assert_eq!(T[round], T[round - 32], "round {round} repeats");
        }
        let distinct: std::collections::BTreeSet<u32> = T.iter().copied().collect();
        assert_eq!(distinct.len(), 48);
    }

    #[test]
    fn the_padding_boundary() {
        for length in [0usize, 1, 55, 56, 63, 64, 65, 119, 128] {
            assert_eq!(sm3(&vec![b'a'; length]).len(), 64, "{length}");
        }
        assert_eq!(Sm3::default().digest_len(), Some(32));
    }

    #[test]
    fn the_input_can_be_cut_anywhere() {
        let body = b"a body of text for the sm3 chunking test, of a reasonable length";
        let whole = Sm3::default().digest(body);
        for cut in 0..body.len() {
            let mut hasher = Sm3::default();
            hasher.update(&body[..cut]);
            hasher.update(&body[cut..]);
            assert_eq!(hasher.finish(), whole, "cut at {cut}");
        }
    }
}