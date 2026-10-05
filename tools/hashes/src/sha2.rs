//! SHA-224 and SHA-256 (FIPS 180-4), and SHA-384 and SHA-512 (FIPS 180-4).
//!
//! The two families differ in more than width: SHA-512 works on 64-bit words,
//! needs a 128-bit length field, and its round constants are the fractional parts
//! of the cube roots of the first eighty primes rather than the square roots. They
//! are written separately here for exactly that reason — a "64-bit variant" of the
//! SHA-256 code is the classic way to get this wrong.
//!
//! The constants are literals rather than computed. The specification gives them as
//! literals, and deriving them at run time has been a source of quiet bugs in
//! several implementations.

use crate::Hasher;

/// The first thirty-two bits of the fractional parts of the square roots of the
/// first eight primes.
const K256: [u32; 64] = [
    0x428a_2f98, 0x7137_4491, 0xb5c0_fbcf, 0xe9b5_dba5, 0x3956_c25b, 0x59f1_11f1, 0x923f_82a4,
    0xab1c_5ed5, 0xd807_aa98, 0x1283_5b01, 0x2431_85be, 0x550c_7dc3, 0x72be_5d74, 0x80de_b1fe,
    0x9bdc_06a7, 0xc19b_f174, 0xe49b_69c1, 0xefbe_4786, 0x0fc1_9dc6, 0x240c_a1cc, 0x2de9_2c6f,
    0x4a74_84aa, 0x5cb0_a9dc, 0x76f9_88da, 0x983e_5152, 0xa831_c66d, 0xb003_27c8, 0xbf59_7fc7,
    0xc6e0_0bf3, 0xd5a7_9147, 0x06ca_6351, 0x1429_2967, 0x27b7_0a85, 0x2e1b_2138, 0x4d2c_6dfc,
    0x5338_0d13, 0x650a_7354, 0x766a_0abb, 0x81c2_c92e, 0x9272_2c85, 0xa2bf_e8a1, 0xa81a_664b,
    0xc24b_8b70, 0xc76c_51a3, 0xd192_e819, 0xd699_0624, 0xf40e_3585, 0x106a_a070, 0x19a4_c116,
    0x1e37_6c08, 0x2748_774c, 0x34b0_bcb5, 0x391c_0cb3, 0x4ed8_aa4a, 0x5b9c_ca4f, 0x682e_6ff3,
    0x748f_82ee, 0x78a5_636f, 0x84c8_7814, 0x8cc7_0208, 0x90be_fffa, 0xa450_6ceb, 0xbef9_a3f7,
    0xc671_78f2,
];

/// The first sixty-four bits of the fractional parts of the cube roots of the first
/// eighty primes.
const K512: [u64; 80] = [
    0x428a2f98d728ae22, 0x7137449123ef65cd, 0xb5c0fbcfec4d3b2f, 0xe9b5dba58189dbbc,
    0x3956c25bf348b538, 0x59f111f1b605d019, 0x923f82a4af194f9b, 0xab1c5ed5da6d8118,
    0xd807aa98a3030242, 0x12835b0145706fbe, 0x243185be4ee4b28c, 0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f, 0x80deb1fe3b1696b1, 0x9bdc06a725c71235, 0xc19bf174cf692694,
    0xe49b69c19ef14ad2, 0xefbe4786384f25e3, 0x0fc19dc68b8cd5b5, 0x240ca1cc77ac9c65,
    0x2de92c6f592b0275, 0x4a7484aa6ea6e483, 0x5cb0a9dcbd41fbd4, 0x76f988da831153b5,
    0x983e5152ee66dfab, 0xa831c66d2db43210, 0xb00327c898fb213f, 0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2, 0xd5a79147930aa725, 0x06ca6351e003826f, 0x142929670a0e6e70,
    0x27b70a8546d22ffc, 0x2e1b21385c26c926, 0x4d2c6dfc5ac42aed, 0x53380d139d95b3df,
    0x650a73548baf63de, 0x766a0abb3c77b2a8, 0x81c2c92e47edaee6, 0x92722c851482353b,
    0xa2bfe8a14cf10364, 0xa81a664bbc423001, 0xc24b8b70d0f89791, 0xc76c51a30654be30,
    0xd192e819d6ef5218, 0xd69906245565a910, 0xf40e35855771202a, 0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8, 0x1e376c085141ab53, 0x2748774cdf8eeb99, 0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63, 0x4ed8aa4ae3418acb, 0x5b9cca4f7763e373, 0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc, 0x78a5636f43172f60, 0x84c87814a1f0ab72, 0x8cc702081a6439ec,
    0x90befffa23631e28, 0xa4506cebde82bde9, 0xbef9a3f7b2c67915, 0xc67178f2e372532b,
    0xca273eceea26619c, 0xd186b8c721c0c207, 0xeada7dd6cde0eb1e, 0xf57d4f7fee6ed178,
    0x06f067aa72176fba, 0x0a637dc5a2c898a6, 0x113f9804bef90dae, 0x1b710b35131c471b,
    0x28db77f523047d84, 0x32caab7b40c72493, 0x3c9ebe0a15c9bebc, 0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6, 0x597f299cfc657e2a, 0x5fcb6fab3ad6faec, 0x6c44198c4a475817,
];

/// SHA-256 compression over 32-bit words.
fn compress256(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for (index, word) in w.iter_mut().take(16).enumerate() {
        let at = index * 4;
        *word = u32::from_be_bytes([block[at], block[at + 1], block[at + 2], block[at + 3]]);
    }
    for index in 16..64 {
        let s0 = w[index - 15].rotate_right(7) ^ w[index - 15].rotate_right(18) ^ (w[index - 15] >> 3);
        let s1 = w[index - 2].rotate_right(17) ^ w[index - 2].rotate_right(19) ^ (w[index - 2] >> 10);
        w[index] = w[index - 16]
            .wrapping_add(s0)
            .wrapping_add(w[index - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for round in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choose = (e & f) ^ (!e & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(choose)
            .wrapping_add(K256[round])
            .wrapping_add(w[round]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }
    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

/// SHA-512 compression over 64-bit words.
fn compress512(state: &mut [u64; 8], block: &[u8; 128]) {
    let mut w = [0u64; 80];
    for (index, word) in w.iter_mut().take(16).enumerate() {
        let at = index * 8;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&block[at..at + 8]);
        *word = u64::from_be_bytes(bytes);
    }
    for index in 16..80 {
        let s0 = w[index - 15].rotate_right(1) ^ w[index - 15].rotate_right(8) ^ (w[index - 15] >> 7);
        let s1 = w[index - 2].rotate_right(19) ^ w[index - 2].rotate_right(61) ^ (w[index - 2] >> 6);
        w[index] = w[index - 16]
            .wrapping_add(s0)
            .wrapping_add(w[index - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for round in 0..80 {
        let s1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);
        let choose = (e & f) ^ (!e & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(choose)
            .wrapping_add(K512[round])
            .wrapping_add(w[round]);
        let s0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }
    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

/// The SHA-256 state with the length a SHA-224 shares, since the two differ only in
/// their initial values and in how much of the final state they print.
macro_rules! sha256_family {
    ($name:ident, $tag:expr, $init:expr, $trunc:expr) => {
        #[doc = concat!("SHA-", $tag, ".")]
        #[derive(Clone, Debug)]
        pub struct $name {
            state: [u32; 8],
            partial: [u8; 64],
            partial_len: usize,
            bits: u64,
        }

        impl Default for $name {
            fn default() -> Self {
                $name {
                    state: $init,
                    partial: [0; 64],
                    partial_len: 0,
                    bits: 0,
                }
            }
        }

        impl Hasher for $name {
            fn update(&mut self, mut bytes: &[u8]) {
                self.bits = self.bits.wrapping_add((bytes.len() as u64) * 8);
                if self.partial_len > 0 {
                    let take = (64 - self.partial_len).min(bytes.len());
                    self.partial[self.partial_len..self.partial_len + take]
                        .copy_from_slice(&bytes[..take]);
                    self.partial_len += take;
                    bytes = &bytes[take..];
                    if self.partial_len == 64 {
                        let block = self.partial;
                        compress256(&mut self.state, &block);
                        self.partial_len = 0;
                    }
                }
                while bytes.len() >= 64 {
                    let mut block = [0u8; 64];
                    block.copy_from_slice(&bytes[..64]);
                    compress256(&mut self.state, &block);
                    bytes = &bytes[64..];
                }
                if !bytes.is_empty() {
                    self.partial[..bytes.len()].copy_from_slice(bytes);
                    self.partial_len = bytes.len();
                }
            }

            fn finish(&mut self) -> Vec<u8> {
                let mut state = self.state;
                let mut block = [0u8; 64];
                block[..self.partial_len].copy_from_slice(&self.partial[..self.partial_len]);
                block[self.partial_len] = 0x80;
                if self.partial_len + 1 + 8 > 64 {
                    compress256(&mut state, &block);
                    block = [0; 64];
                }
                block[56..].copy_from_slice(&self.bits.to_be_bytes());
                compress256(&mut state, &block);
                let mut out = Vec::with_capacity($trunc);
                for word in state.iter().take($trunc / 4) {
                    out.extend_from_slice(&word.to_be_bytes());
                }
                out
            }

            fn digest_len(&self) -> Option<usize> {
                Some($trunc)
            }
        }

        impl $name {
            /// The starting state, so a test can see what the macro was handed.
            pub fn starting_state(&self) -> [u32; 8] {
                self.state
            }
        }
    };
}

sha256_family!(Sha224, "224", [
    0xc105_9ed8, 0x367c_d507, 0x3070_dd17, 0xf70e_5939, 0xffc0_0b31, 0x6858_1511, 0x64f9_8fa7,
    0xbefa_4fa4
], 28);
sha256_family!(Sha256, "256", [
    0x6a09_e667, 0xbb67_ae85, 0x3c6e_f372, 0xa54f_f53a, 0x510e_527f, 0x9b05_688c, 0x1f83_d9ab,
    0x5be0_cd19
], 32);

macro_rules! sha512_family {
    ($name:ident, $tag:expr, $init:expr, $trunc:expr) => {
        #[doc = concat!("SHA-", $tag, ".")]
        #[derive(Clone, Debug)]
        pub struct $name {
            state: [u64; 8],
            partial: [u8; 128],
            partial_len: usize,
            bits: u128,
        }

        impl Default for $name {
            fn default() -> Self {
                $name {
                    state: $init,
                    partial: [0; 128],
                    partial_len: 0,
                    bits: 0,
                }
            }
        }

        impl Hasher for $name {
            fn update(&mut self, mut bytes: &[u8]) {
                self.bits = self.bits.wrapping_add((bytes.len() as u128) * 8);
                if self.partial_len > 0 {
                    let take = (128 - self.partial_len).min(bytes.len());
                    self.partial[self.partial_len..self.partial_len + take]
                        .copy_from_slice(&bytes[..take]);
                    self.partial_len += take;
                    bytes = &bytes[take..];
                    if self.partial_len == 128 {
                        let block = self.partial;
                        compress512(&mut self.state, &block);
                        self.partial_len = 0;
                    }
                }
                while bytes.len() >= 128 {
                    let mut block = [0u8; 128];
                    block.copy_from_slice(&bytes[..128]);
                    compress512(&mut self.state, &block);
                    bytes = &bytes[128..];
                }
                if !bytes.is_empty() {
                    self.partial[..bytes.len()].copy_from_slice(bytes);
                    self.partial_len = bytes.len();
                }
            }

            fn finish(&mut self) -> Vec<u8> {
                let mut state = self.state;
                let mut block = [0u8; 128];
                block[..self.partial_len].copy_from_slice(&self.partial[..self.partial_len]);
                block[self.partial_len] = 0x80;
                if self.partial_len + 1 + 16 > 128 {
                    compress512(&mut state, &block);
                    block = [0; 128];
                }
                // The count is 128 bits here, which is the one structural difference
                // from the 32-bit family besides the word size.
                block[112..].copy_from_slice(&self.bits.to_be_bytes());
                compress512(&mut state, &block);
                let mut out = Vec::with_capacity($trunc);
                for word in state.iter().take($trunc / 8) {
                    out.extend_from_slice(&word.to_be_bytes());
                }
                out
            }

            fn digest_len(&self) -> Option<usize> {
                Some($trunc)
            }
        }

        impl $name {
            /// The starting state, so a test can see what the macro was handed.
            pub fn starting_state(&self) -> [u64; 8] {
                self.state
            }
        }
    };
}

sha512_family!(Sha384, "384", [
    0xcbbb_9d5d_c105_9ed8, 0x629a_292a_367c_d507, 0x9159_015a_3070_dd17, 0x152f_ecd8_f70e_5939,
    0x6733_2667_ffc0_0b31, 0x8eb4_4a87_6858_1511, 0xdb0c_2e0d_64f9_8fa7, 0x47b5_481d_befa_4fa4
], 48);
sha512_family!(Sha512, "512", [
    0x6a09_e667_f3bc_c908, 0xbb67_ae85_84ca_a73b, 0x3c6e_f372_fe94_f82b, 0xa54f_f53a_5f1d_36f1,
    0x510e_527f_ade6_82d1, 0x9b05_688c_2b3e_6c1f, 0x1f83_d9ab_fb41_bd6b, 0x5be0_cd19_137e_2179
], 64);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::to_hex;

    #[test]
    fn the_fips180_examples_for_sha224_and_sha256() {
        assert_eq!(to_hex(&Sha224::default().digest(b"")), "d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f");
        assert_eq!(to_hex(&Sha224::default().digest(b"abc")), "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7");
        assert_eq!(to_hex(&Sha256::default().digest(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(to_hex(&Sha256::default().digest(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let two_block = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(to_hex(&Sha224::default().digest(two_block)), "75388b16512776cc5dba5da1fd890150b0c6455cb4f58b1952522525");
        assert_eq!(to_hex(&Sha256::default().digest(two_block)), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1");
    }

    #[test]
    fn the_fips180_examples_for_sha384_and_sha512() {
        assert_eq!(to_hex(&Sha384::default().digest(b"")), "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b");
        assert_eq!(to_hex(&Sha384::default().digest(b"abc")), "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7");
        assert_eq!(to_hex(&Sha512::default().digest(b"")), "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e");
        assert_eq!(to_hex(&Sha512::default().digest(b"abc")), "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f");
        let two_block = b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu";
        assert_eq!(to_hex(&Sha384::default().digest(two_block)), "09330c33f71147e83d192fc782cd1b4753111b173b3b05d22fa08086e3b0f712fcc7c71a557e2db966c3e9fa91746039");
    }

    #[test]
    fn a_million_a_for_the_32_bit_family() {
        let mut hasher = Sha256::default();
        for _ in 0..1000 {
            hasher.update(&[b'a'; 1000]);
        }
        assert_eq!(to_hex(&hasher.finish()), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
    }

    #[test]
    fn the_starting_states_are_the_specified_ones() {
        // A single wrong digit in an initial value produces a digest of exactly the
        // right length that nothing else agrees with, and no published example
        // covers every one of the sixteen of them. These are the four states from
        // FIPS 180-4, spelled out.
        assert_eq!(
            Sha224::default().starting_state(),
            [
                0xc105_9ed8, 0x367c_d507, 0x3070_dd17, 0xf70e_5939, 0xffc0_0b31, 0x6858_1511,
                0x64f9_8fa7, 0xbefa_4fa4
            ]
        );
        assert_eq!(
            Sha256::default().starting_state(),
            [
                0x6a09_e667, 0xbb67_ae85, 0x3c6e_f372, 0xa54f_f53a, 0x510e_527f, 0x9b05_688c,
                0x1f83_d9ab, 0x5be0_cd19
            ]
        );
        assert_eq!(
            Sha384::default().starting_state(),
            [
                0xcbbb_9d5d_c105_9ed8,
                0x629a_292a_367c_d507,
                0x9159_015a_3070_dd17,
                0x152f_ecd8_f70e_5939,
                0x6733_2667_ffc0_0b31,
                0x8eb4_4a87_6858_1511,
                0xdb0c_2e0d_64f9_8fa7,
                0x47b5_481d_befa_4fa4
            ]
        );
        assert_eq!(
            Sha512::default().starting_state(),
            [
                0x6a09_e667_f3bc_c908,
                0xbb67_ae85_84ca_a73b,
                0x3c6e_f372_fe94_f82b,
                0xa54f_f53a_5f1d_36f1,
                0x510e_527f_ade6_82d1,
                0x9b05_688c_2b3e_6c1f,
                0x1f83_d9ab_fb41_bd6b,
                0x5be0_cd19_137e_2179
            ]
        );
    }

    #[test]
    fn the_truncation_is_where_it_should_be() {
        // SHA-224 is not SHA-256 with its last four bytes dropped; it starts from a
        // different state, so only the length is shared.
        assert_ne!(
            to_hex(&Sha224::default().digest(b"abc"))[..56],
            to_hex(&Sha256::default().digest(b"abc"))[..56]
        );
        assert_eq!(Sha224::default().digest_len(), Some(28));
        assert_eq!(Sha256::default().digest_len(), Some(32));
        assert_eq!(Sha384::default().digest_len(), Some(48));
        assert_eq!(Sha512::default().digest_len(), Some(64));
    }

    #[test]
    fn the_input_can_be_cut_anywhere() {
        let body = b"a body of text for the sha2 chunking test, of a reasonable length";
        // One algorithm per expansion, so a failure says which one broke.
        macro_rules! cut_test {
            ($type:ty) => {{
                let whole = <$type>::default().digest(body);
                for cut in 0..body.len() {
                    let mut hasher = <$type>::default();
                    hasher.update(&body[..cut]);
                    hasher.update(&body[cut..]);
                    assert_eq!(hasher.finish(), whole, "{} cut at {cut}", stringify!($type));
                }
            }};
        }
        cut_test!(Sha224);
        cut_test!(Sha256);
        cut_test!(Sha384);
        cut_test!(Sha512);
    }

    #[test]
    fn the_padding_boundary_for_both_block_sizes() {
        // 32-bit family: 55 fits, 56 does not. 64-bit family: 111 fits, 112 does not.
        for length in [0usize, 55, 56, 63, 64, 111, 112, 119, 127, 128, 239, 240] {
            assert_eq!(Sha256::default().digest(&vec![b'a'; length]).len(), 32, "sha256 {length}");
            assert_eq!(Sha512::default().digest(&vec![b'a'; length]).len(), 64, "sha512 {length}");
        }
    }
}