//! The two 16-bit checksums that `sum` and `cksum` call bsd and sysv, plus the
//! CRC-32 that `cksum` prints by default.
//!
//! None of these is a cryptographic digest and none of them was designed to be:
//! the BSD one is a rotate-and-add, the SysV one a plain sum of bytes, and the
//! CRC-32 the one that actually detects burst errors. They are here because `sum`
//! and `cksum` have to produce them, not because they are worth using.
//!
//! The CRC is the surprise. It is **not** the CRC-32 that zlib, PNG and Ethernet
//! use, even though that one is the one almost everybody means by "CRC-32". The
//! three differences are all load-bearing:
//!
//! * polynomial `0x04c11db7`, processed most-significant-bit first, rather than the
//!   reflected `0xedb88320` update the reflected form uses;
//! * an initial value of zero rather than all ones;
//! * and the file's **length** is appended to the checksum, least significant byte
//!   first, before the result is complemented.
//!
//! Any one of those left out gives a plausible-looking number that disagrees with
//! `cksum`, so the three are together in [`Crc32::finish_with_length`] where the
//! comment can point at them.

use crate::Hasher;

/// CRC-32 as `cksum` computes it: polynomial `0x04c11db7`, initial value zero, no
/// final xor, and the input length folded in at the end.
///
/// This is the same function the CRC catalogue calls **CRC-32/BZIP2**, up to that
/// last step. Getting it from the name "CRC-32" alone is how tools end up emitting
/// a checksum that disagrees with every other one in the name.
#[derive(Clone, Debug)]
pub struct Crc32 {
    state: u32,
}

impl Default for Crc32 {
    fn default() -> Self {
        Crc32 { state: 0 }
    }
}

impl Crc32 {
    /// The remainder table, built once: the remainder of each byte value shifted up
    /// to the top of the word and divided by the polynomial.
    fn table() -> &'static [u32; 256] {
        use std::sync::OnceLock;
        static TABLE: OnceLock<[u32; 256]> = OnceLock::new();
        TABLE.get_or_init(|| {
            const POLYNOMIAL: u32 = 0x04c1_1db7;
            let mut table = [0u32; 256];
            for (index, slot) in table.iter_mut().enumerate() {
                let mut remainder = (index as u32) << 24;
                for _ in 0..8 {
                    remainder = if remainder & 0x8000_0000 != 0 {
                        (remainder << 1) ^ POLYNOMIAL
                    } else {
                        remainder << 1
                    };
                }
                *slot = remainder;
            }
            table
        })
    }

    /// Fold one more byte in. MSB-first, which is what makes this a different
    /// function from the reflected update rather than the same one spelled
    /// differently.
    fn absorb(state: &mut u32, byte: u8) {
        let table = Self::table();
        *state = (*state << 8) ^ table[(((*state >> 24) ^ u32::from(byte)) & 0xff) as usize];
    }
}

impl Hasher for Crc32 {
    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            Self::absorb(&mut self.state, *byte);
        }
    }

    fn finish(&mut self) -> Vec<u8> {
        // Without the length this is still well defined — it is the checksum of the
        // data as if it were zero bytes long — but nothing asks for it.
        self.finish_with_length(0)
    }

    fn finish_with_length(&mut self, length: u64) -> Vec<u8> {
        // The length goes in as though it were more input, most significant byte
        // last, and then the whole thing is complemented. An empty file has nothing
        // to append, which is why its checksum is all ones.
        let mut remaining = length;
        while remaining != 0 {
            Self::absorb(&mut self.state, (remaining & 0xff) as u8);
            remaining >>= 8;
        }
        (!self.state).to_be_bytes().to_vec()
    }

    fn digest_len(&self) -> Option<usize> {
        Some(4)
    }
}

/// `sum -r`: rotate right one bit, then add the byte, modulo 2^16.
#[derive(Clone, Debug, Default)]
pub struct BsdSum {
    checksum: u16,
}

impl BsdSum {
    /// The checksum as the number `sum` prints, which is the whole 16-bit value.
    ///
    /// The raw form is the same number in network byte order, so a caller that wants
    /// bytes has [`Hasher::finish`]; this is here because the numeric print and the
    /// raw print are separate code paths in the tool and one of them wants the
    /// integer.
    pub fn value(&self) -> u16 {
        self.checksum
    }
}

impl Hasher for BsdSum {
    fn update(&mut self, bytes: &[u8]) {
        let mut checksum = self.checksum;
        for byte in bytes {
            checksum = (checksum >> 1) | (checksum << 15);
            checksum = checksum.wrapping_add(u16::from(*byte));
        }
        self.checksum = checksum;
    }

    fn finish(&mut self) -> Vec<u8> {
        // `sum` prints this in network byte order even on a little-endian machine,
        // so the raw form is big-endian rather than whatever the register is.
        self.checksum.to_be_bytes().to_vec()
    }

    fn digest_len(&self) -> Option<usize> {
        Some(2)
    }
}

/// `sum -s`: the sum of every byte, modulo 2^32.
#[derive(Clone, Debug, Default)]
pub struct SysvSum {
    sum: u32,
}

impl SysvSum {
    /// The sum as the number `sum` prints.
    ///
    /// Not the running total: a 32-bit accumulator is folded into sixteen bits with
    /// an end-around carry, so a 65536-byte file of one-byte values sums to 1
    /// rather than to 0. That is the one's-complement sum the SysV tool has always
    /// computed, and getting it wrong shows up only past the 65535th byte — which
    /// is why a file under 64 KiB is a poor test of it.
    pub fn value(&self) -> u16 {
        let folded = (self.sum & 0xffff).wrapping_add(self.sum >> 16);
        (folded & 0xffff).wrapping_add(folded >> 16) as u16
    }
}

impl Hasher for SysvSum {
    fn update(&mut self, bytes: &[u8]) {
        // Accumulated in u32 and allowed to wrap: the source sums into an `unsigned
        // int` and relies on the overflow, so this is the same arithmetic.
        let mut total: u32 = 0;
        for byte in bytes {
            total = total.wrapping_add(u32::from(*byte));
        }
        self.sum = self.sum.wrapping_add(total);
    }

    fn finish(&mut self) -> Vec<u8> {
        self.value().to_be_bytes().to_vec()
    }

    fn digest_len(&self) -> Option<usize> {
        Some(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::to_hex;

    const FOX: &[u8] = b"The quick brown fox jumps over the lazy dog";

    /// Everything expected here was read off `cksum` or `sum`.
    fn cksum_crc(bytes: &[u8]) -> String {
        to_hex(&Crc32::default().digest(bytes))
    }

    #[test]
    fn the_crc_is_not_the_one_everybody_else_calls_crc32() {
        // This is the point of the whole module, so it is the first thing tested:
        // the zlib/PNG/Ethernet CRC-32 of the same input, which shares a name and
        // nothing else that matters.
        let ours = cksum_crc(b"hello\n");
        let zlib = {
            // The reflected variant, computed here so the difference is visible
            // rather than asserted.
            fn reflected(data: &[u8]) -> u32 {
                let mut state = 0xffff_ffffu32;
                for byte in data {
                    state ^= u32::from(*byte);
                    for _ in 0..8 {
                        state = if state & 1 != 0 {
                            (state >> 1) ^ 0xedb8_8320
                        } else {
                            state >> 1
                        };
                    }
                }
                !state
            }
            format!("{:08x}", reflected(b"hello\n"))
        };
        assert_eq!(ours, "b3beab91");
        assert_eq!(zlib, "363a3020");
        assert_ne!(ours, zlib, "this is the whole point of the note above");
    }

    #[test]
    fn the_crc_folds_the_length_in() {
        // cksum reports 2074844392 for the 43-byte fox, and 3015617425 for "hello\n".
        assert_eq!(cksum_crc(FOX), "7bab9ce8");
        assert_eq!(cksum_crc(b"hello\n"), "b3beab91");
        assert_eq!(cksum_crc(b""), "ffffffff");
        // An empty file has no length to append, so the complement of zero is left.
        assert_eq!(u32::from_be_bytes(Crc32::default().digest(b"").try_into().unwrap()), 0xffff_ffff);
    }

    #[test]
    fn the_crc_is_the_same_however_the_input_is_cut_up() {
        let whole = Crc32::default().digest(FOX);
        for cut in 0..FOX.len() {
            let mut hasher = Crc32::default();
            hasher.update(&FOX[..cut]);
            hasher.update(&FOX[cut..]);
            assert_eq!(hasher.finish_with_length(FOX.len() as u64), whole, "cut at {cut}");
        }
    }

    #[test]
    fn the_sums_are_what_sum_prints() {
        // "hello\n" is 104 + 101 + 108 + 108 + 111 + 10.
        assert_eq!(SysvSum::default().digest(b"hello\n"), 542u16.to_be_bytes());
        assert_eq!(BsdSum::default().digest(b"hello\n"), 36979u16.to_be_bytes());
        assert_eq!(SysvSum::default().digest(FOX), 4057u16.to_be_bytes());
        assert_eq!(BsdSum::default().digest(FOX), 50542u16.to_be_bytes());
        assert_eq!(SysvSum::default().digest(b""), [0, 0]);
        assert_eq!(BsdSum::default().digest(b""), [0, 0]);
    }

    #[test]
    fn the_sysv_sum_folds_with_an_end_around_carry() {
        // 65536 bytes of one sums to 1, not to 0, which is only visible past the
        // 65535th byte and is the difference between this and a plain byte sum.
        let ones = |n: usize| {
            let mut hasher = SysvSum::default();
            hasher.update(&vec![1u8; n]);
            hasher.value()
        };
        assert_eq!(ones(1000), 1000);
        assert_eq!(ones(65535), 65535);
        assert_eq!(ones(65536), 1);
        assert_eq!(ones(65537), 2);
        assert_eq!(ones(131072), 2);
        assert_eq!(ones(70000), 4465);
        assert_eq!(ones(200000), 3395);
        // The same inputs through cksum, which is where these numbers came from.
        let mut hasher = SysvSum::default();
        hasher.update(&vec![0xa5u8; 100_000]);
        assert_eq!(hasher.value(), 50715);
        assert_eq!(SysvSum::default().digest(&vec![0xa5u8; 100_000]).len(), 2);
    }

    #[test]
    fn the_sums_have_a_number_and_a_raw_form() {
        let mut sysv = SysvSum::default();
        sysv.update(FOX);
        assert_eq!(sysv.value(), 4057);
        assert_eq!(sysv.finish().len(), 2);
        let mut bsd = BsdSum::default();
        bsd.update(FOX);
        assert_eq!(bsd.value(), 50542);
        assert_eq!(bsd.finish().len(), 2);
    }

    #[test]
    fn the_sums_wrap_instead_of_overflowing() {
        // 100 bytes of 0xff overflows a 16-bit sum many times over, and both sums
        // are defined modulo their width rather than saturating.
        // cksum says 25500 and 57693 for these two.
        assert_eq!(to_hex(&SysvSum::default().digest(&[0xff; 100])), "639c");
        assert_eq!(to_hex(&BsdSum::default().digest(&[0xff; 100])), "e15d");
        // A long run still terminates and still wraps rather than panicking.
        assert_eq!(to_hex(&SysvSum::default().digest(&vec![0xa5u8; 100_000])), "c61b");
        assert_eq!(to_hex(&BsdSum::default().digest(&vec![0xa5u8; 100_000])), "afc9");
    }

    #[test]
    fn the_digest_lengths_are_what_the_tools_expect() {
        assert_eq!(Crc32::default().digest_len(), Some(4));
        assert_eq!(BsdSum::default().digest_len(), Some(2));
        assert_eq!(SysvSum::default().digest_len(), Some(2));
    }
}