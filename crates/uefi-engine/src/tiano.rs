//! Порт refs/UEFITool-ai-fork/common/Tiano/EfiTianoDecompress.c — единый
//! LZ77+Huffman-декодер EFI 1.1 (pbit=4) и Tiano (pbit=5).
//! Спека: docs/superpowers/specs/2026-09-20-tiano-op-design.md §1.
//! Отличия от C-эталона (поведение на валидных данных идентично): walk
//! по дереву Хаффмана ограничен глубиной битовой маски (битый поток →
//! Corrupted вместо вечного цикла); orig_size > 512 МиБ → Corrupted;
//! get_bits(0) возвращает 0 (в C не вызывается, `>> 32` в Rust паникует);
//! UINT16-переполнение mBlockSize на завершающем «фантомном» DecodeC —
//! wrapping, как в C ( ref: DecodeC, C:673); read_clen пробрасывает ошибку
//! построения C-таблицы, которую C-эталон игнорирует ( ref:
//! EfiTianoDecompress.c:612); на битых данных Rust строже.

use crate::decompress::DecompressError;

const BITBUF: u32 = 32;
const THRESHOLD: u32 = 3;
/// NC = 0xff + MAXMATCH(256) + 2 - THRESHOLD(3)
const NC: usize = 510;
const CBIT: u32 = 9;
const MAXPBIT: u32 = 5;
const TBIT: u32 = 5;
/// MAXNP = (1 << MAXPBIT) - 1
const MAXNP: usize = (1 << MAXPBIT) - 1;
/// NT = CODE_BIT(16) + 3
const NT: usize = 19;
const NPT: usize = if NT > MAXNP { NT } else { MAXNP };
const MAX_ORIG_SIZE: u32 = 512 * 1024 * 1024;

/// Вариант алгоритма: EFI 1.1 standard (pbit=4) или Tiano (pbit=5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pbit {
    Efi,
    Tiano,
}

#[derive(Clone, Copy)]
enum Which {
    Pt,
    Cl,
}

/// C-указатель `Pointer` из MakeTable гуляет между Table и mLeft/mRight;
/// моделируем локацией (ref: MakeTable).
#[derive(Clone, Copy)]
enum Slot {
    Table(usize),
    Left(usize),
    Right(usize),
}

struct Decoder {
    src: Vec<u8>,
    in_buf: usize,
    comp_size: u32,
    orig_size: usize,
    out: Vec<u8>,
    out_pos: usize,
    bit_count: u16,
    bit_buf: u32,
    sub_bit_buf: u32,
    block_size: u16,
    left: [u16; 2 * NC - 1],
    right: [u16; 2 * NC - 1],
    c_len: [u8; NC],
    pt_len: [u8; NPT],
    c_table: [u16; 4096],
    pt_table: [u16; 256],
    p_bit: u16,
}

impl Decoder {
    /// ref: FillBuf. Сдвиги через u64: C-эталон делает LShiftU64 (в т.ч.
    /// на 32 при инициализации), прямой `u32 << 32` в Rust паникует.
    fn fill_buf(&mut self, mut num_bits: u16) {
        self.bit_buf = (u64::from(self.bit_buf) << num_bits) as u32;
        while num_bits > self.bit_count {
            num_bits -= self.bit_count;
            self.bit_buf |= (u64::from(self.sub_bit_buf) << num_bits) as u32;
            if self.comp_size > 0 {
                self.comp_size -= 1;
                self.sub_bit_buf = u32::from(self.src[self.in_buf]);
                self.in_buf += 1;
                self.bit_count = 8;
            } else {
                // За концом источника эталон паддит нулевыми битами.
                self.sub_bit_buf = 0;
                self.bit_count = 8;
            }
        }
        self.bit_count -= num_bits;
        self.bit_buf |= self.sub_bit_buf >> self.bit_count;
    }

    /// ref: GetBits
    fn get_bits(&mut self, num_bits: u16) -> u32 {
        if num_bits == 0 {
            return 0;
        }
        let out = self.bit_buf >> (BITBUF - u32::from(num_bits));
        self.fill_buf(num_bits);
        out
    }

    fn slot_get(&self, slot: Slot, which: Which) -> u16 {
        match slot {
            Slot::Table(i) => match which {
                Which::Pt => self.pt_table[i],
                Which::Cl => self.c_table[i],
            },
            Slot::Left(i) => self.left[i],
            Slot::Right(i) => self.right[i],
        }
    }

    fn slot_set(&mut self, slot: Slot, which: Which, value: u16) {
        match slot {
            Slot::Table(i) => match which {
                Which::Pt => self.pt_table[i] = value,
                Which::Cl => self.c_table[i] = value,
            },
            Slot::Left(i) => self.left[i] = value,
            Slot::Right(i) => self.right[i] = value,
        }
    }

    /// ref: MakeTable. `BitLen` — pt_len или c_len в зависимости от `which`.
    fn make_table(
        &mut self,
        num_of_char: u16,
        table_bits: u16,
        which: Which,
    ) -> Result<(), DecompressError> {
        // C-эталон отвергает >= 17; здесь 16 тоже отбрасывается: живых вызовов
        // с 16 нет (только 8/12), а `1u16 << (15 - 16)` в Rust паникует.
        if table_bits >= 16 {
            return Err(DecompressError::Corrupted);
        }
        let bit_len: Vec<u8> = match which {
            Which::Pt => self.pt_len.to_vec(),
            Which::Cl => self.c_len.to_vec(),
        };
        let mut count = [0u16; 17];
        let mut weight = [0u16; 17];
        let mut start = [0u16; 18];

        for &len in &bit_len[..usize::from(num_of_char)] {
            let len = usize::from(len);
            if len > 16 {
                return Err(DecompressError::Corrupted);
            }
            count[len] += 1;
        }

        for index in 1..=16usize {
            start[index + 1] = start[index].wrapping_add(count[index] << (16 - index));
        }
        if start[17] != 0 {
            return Err(DecompressError::Corrupted);
        }

        let ju_bits = 16 - table_bits;
        for index in 1..=usize::from(table_bits) {
            start[index] >>= ju_bits;
            weight[index] = 1u16 << (usize::from(table_bits) - index);
        }
        for (index, slot) in weight
            .iter_mut()
            .enumerate()
            .skip(usize::from(table_bits) + 1)
        {
            *slot = 1u16 << (16 - index);
        }

        let table_max = 1usize << table_bits;
        let zero_from = usize::from(start[usize::from(table_bits) + 1] >> ju_bits);
        if zero_from != 0 && zero_from < table_max {
            match which {
                Which::Pt => self.pt_table[zero_from..].fill(0),
                Which::Cl => self.c_table[zero_from..].fill(0),
            }
        }

        let mut avail = num_of_char;
        let mask = 1u16 << (15 - table_bits);

        for (char_idx, &len) in bit_len[..usize::from(num_of_char)].iter().enumerate() {
            let len = u16::from(len);
            if len == 0 || len >= 17 {
                continue;
            }
            let li = usize::from(len);
            let next_code = start[li].wrapping_add(weight[li]);

            if len <= table_bits {
                let mut idx = usize::from(start[li]);
                while idx < usize::from(next_code) {
                    if idx >= table_max {
                        return Err(DecompressError::Corrupted);
                    }
                    self.slot_set(Slot::Table(idx), which, char_idx as u16);
                    idx += 1;
                }
            } else {
                let mut index3 = u32::from(start[li]);
                let mut pointer = Slot::Table((index3 >> ju_bits) as usize);
                let mut depth = len - table_bits;
                while depth != 0 {
                    let cur = self.slot_get(pointer, which);
                    if cur == 0 && usize::from(avail) < 2 * NC - 1 {
                        self.right[usize::from(avail)] = 0;
                        self.left[usize::from(avail)] = 0;
                        self.slot_set(pointer, which, avail);
                        avail += 1;
                    }
                    let cur = self.slot_get(pointer, which);
                    if cur < (2 * NC - 1) as u16 {
                        pointer = if (index3 & u32::from(mask)) != 0 {
                            Slot::Right(usize::from(cur))
                        } else {
                            Slot::Left(usize::from(cur))
                        };
                    }
                    index3 <<= 1;
                    depth -= 1;
                }
                self.slot_set(pointer, which, char_idx as u16);
            }
            start[li] = next_code;
        }
        Ok(())
    }

    /// ref: ReadPTLen. `special` — символ, после которого идёт счётчик нулей
    /// (3 для exTra Set; u16::MAX = «никогда» для Position Set).
    fn read_pt_len(&mut self, nn: u16, nbit: u16, special: u16) -> Result<(), DecompressError> {
        let number = self.get_bits(nbit) as u16;
        if usize::from(number) > NPT || usize::from(nn) > NPT {
            return Err(DecompressError::Corrupted);
        }
        if number == 0 {
            let char_c = self.get_bits(nbit) as u16;
            self.pt_table = [char_c; 256];
            self.pt_len[..usize::from(nn)].fill(0);
            return Ok(());
        }

        let mut index = 0usize;
        while index < usize::from(number) && index < NPT {
            let mut char_c = (self.bit_buf >> (BITBUF - 3)) as u16;
            if char_c == 7 {
                let mut mask = 1u32 << (BITBUF - 1 - 3);
                while (mask & self.bit_buf) != 0 {
                    mask >>= 1;
                    char_c += 1;
                }
            }
            self.fill_buf(if char_c < 7 { 3 } else { char_c - 3 });
            self.pt_len[index] = char_c as u8;
            index += 1;

            if index as u16 == special {
                let zeros = self.get_bits(2) as u16;
                for _ in 0..zeros {
                    if index >= NPT {
                        break;
                    }
                    self.pt_len[index] = 0;
                    index += 1;
                }
            }
        }
        while index < usize::from(nn) && index < NPT {
            self.pt_len[index] = 0;
            index += 1;
        }
        self.make_table(nn, 8, Which::Pt)
    }

    /// ref: ReadCLen
    fn read_clen(&mut self) -> Result<(), DecompressError> {
        let number = self.get_bits(CBIT as u16) as u16;
        if number == 0 {
            let char_c = self.get_bits(CBIT as u16) as u16;
            self.c_len = [0; NC];
            self.c_table = [char_c; 4096];
            return Ok(());
        }

        let mut index = 0usize;
        while index < usize::from(number) && index < NC {
            let mut char_c = self.pt_table[(self.bit_buf >> (BITBUF - 8)) as usize];
            if char_c >= NT as u16 {
                let mut mask = 1u32 << (BITBUF - 1 - 8);
                loop {
                    char_c = if (mask & self.bit_buf) != 0 {
                        self.right[usize::from(char_c)]
                    } else {
                        self.left[usize::from(char_c)]
                    };
                    mask >>= 1;
                    if char_c < NT as u16 {
                        break;
                    }
                    if mask == 0 {
                        return Err(DecompressError::Corrupted);
                    }
                }
            }
            self.fill_buf(u16::from(self.pt_len[usize::from(char_c)]));

            if char_c <= 2 {
                let zeros = if char_c == 0 {
                    1
                } else if char_c == 1 {
                    self.get_bits(4) as u16 + 3
                } else {
                    self.get_bits(CBIT as u16) as u16 + 20
                };
                for _ in 0..zeros {
                    if index >= NC {
                        break;
                    }
                    self.c_len[index] = 0;
                    index += 1;
                }
            } else {
                self.c_len[index] = (char_c - 2) as u8;
                index += 1;
            }
        }
        for i in index..NC {
            self.c_len[i] = 0;
        }
        self.make_table(NC as u16, 12, Which::Cl)
    }

    /// ref: DecodeC
    fn decode_c(&mut self) -> Result<u16, DecompressError> {
        if self.block_size == 0 {
            self.block_size = self.get_bits(16) as u16;
            self.read_pt_len(NT as u16, TBIT as u16, 3)?;
            self.read_clen()?;
            self.read_pt_len(MAXNP as u16, self.p_bit, u16::MAX)?;
        }
        self.block_size = self.block_size.wrapping_sub(1);

        let mut index2 = self.c_table[(self.bit_buf >> (BITBUF - 12)) as usize];
        if index2 >= NC as u16 {
            let mut mask = 1u32 << (BITBUF - 1 - 12);
            loop {
                index2 = if (mask & self.bit_buf) != 0 {
                    self.right[usize::from(index2)]
                } else {
                    self.left[usize::from(index2)]
                };
                mask >>= 1;
                if index2 < NC as u16 {
                    break;
                }
                if mask == 0 {
                    return Err(DecompressError::Corrupted);
                }
            }
        }
        self.fill_buf(u16::from(self.c_len[usize::from(index2)]));
        Ok(index2)
    }

    /// ref: DecodeP
    fn decode_p(&mut self) -> Result<u32, DecompressError> {
        let mut val = self.pt_table[(self.bit_buf >> (BITBUF - 8)) as usize];
        if val >= MAXNP as u16 {
            let mut mask = 1u32 << (BITBUF - 1 - 8);
            loop {
                val = if (mask & self.bit_buf) != 0 {
                    self.right[usize::from(val)]
                } else {
                    self.left[usize::from(val)]
                };
                mask >>= 1;
                if val < MAXNP as u16 {
                    break;
                }
                if mask == 0 {
                    return Err(DecompressError::Corrupted);
                }
            }
        }
        self.fill_buf(u16::from(self.pt_len[usize::from(val)]));

        if val > 1 {
            let extra = u32::from(val) - 1;
            return Ok((1u32 << extra) + self.get_bits(extra as u16));
        }
        Ok(u32::from(val))
    }

    /// ref: Decode
    fn decode(&mut self) -> Result<(), DecompressError> {
        loop {
            let char_c = self.decode_c()?;
            if char_c < 256 {
                if self.out_pos >= self.orig_size {
                    return Ok(());
                }
                self.out[self.out_pos] = char_c as u8;
                self.out_pos += 1;
            } else {
                let len = u32::from(char_c) - (0x100 - THRESHOLD);
                let pos = self.decode_p()?;
                for data_idx in
                    ((self.out_pos as u32).wrapping_sub(pos).wrapping_sub(1)..).take(len as usize)
                {
                    if self.out_pos >= self.orig_size {
                        return Ok(());
                    }
                    if data_idx >= self.orig_size as u32 {
                        return Err(DecompressError::Corrupted);
                    }
                    self.out[self.out_pos] = self.out[data_idx as usize];
                    self.out_pos += 1;
                }
                if self.out_pos >= self.orig_size {
                    return Ok(());
                }
            }
        }
    }
}

/// Разворачивает payload секции (заголовок EFI_TIANO_HEADER + поток).
/// Битый вход → `DecompressError::Corrupted`; пустой (orig_size=0) →
/// пустой выход. pbit: EFI 1.1 standard = 4, Tiano = 5 (ref: Decompress).
pub fn decompress(data: &[u8], pbit: Pbit) -> Result<Vec<u8>, DecompressError> {
    if data.len() < 8 {
        return Err(DecompressError::Corrupted);
    }
    let comp_size = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let orig_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    if orig_size == 0 {
        return Ok(Vec::new());
    }
    if orig_size > MAX_ORIG_SIZE {
        return Err(DecompressError::Corrupted);
    }
    let Some(total) = comp_size.checked_add(8) else {
        return Err(DecompressError::Corrupted);
    };
    if u64::from(total) > data.len() as u64 {
        return Err(DecompressError::Corrupted);
    }

    let mut decoder = Decoder {
        src: data[8..].to_vec(),
        in_buf: 0,
        comp_size,
        orig_size: orig_size as usize,
        out: vec![0u8; orig_size as usize],
        out_pos: 0,
        bit_count: 0,
        bit_buf: 0,
        sub_bit_buf: 0,
        block_size: 0,
        left: [0; 2 * NC - 1],
        right: [0; 2 * NC - 1],
        c_len: [0; NC],
        pt_len: [0; NPT],
        c_table: [0; 4096],
        pt_table: [0; 256],
        p_bit: match pbit {
            Pbit::Efi => 4,
            Pbit::Tiano => 5,
        },
    };
    decoder.fill_buf(BITBUF as u16);
    decoder.decode()?;
    Ok(decoder.out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN_IN: &[u8] = include_bytes!("../tests/fixtures/tiano/c275-min.in");
    const MIN_EXPECTED: &[u8] = include_bytes!("../tests/fixtures/tiano/c275-min.expected");
    const MEDIAN_IN: &[u8] = include_bytes!("../tests/fixtures/tiano/c275-median.in");
    const MEDIAN_EXPECTED: &[u8] = include_bytes!("../tests/fixtures/tiano/c275-median.expected");
    const MAX_IN: &[u8] = include_bytes!("../tests/fixtures/tiano/c275-max.in");
    const MAX_EXPECTED: &[u8] = include_bytes!("../tests/fixtures/tiano/c275-max.expected");

    #[test]
    fn input_shorter_than_header_is_corrupted() {
        assert!(matches!(
            decompress(&[1, 2, 3], Pbit::Efi),
            Err(DecompressError::Corrupted)
        ));
    }

    #[test]
    fn lying_comp_size_is_corrupted() {
        let mut data = vec![0u8; 16];
        data[0..4].copy_from_slice(&u32::MAX.to_le_bytes());
        data[4..8].copy_from_slice(&16u32.to_le_bytes());
        assert!(matches!(
            decompress(&data, Pbit::Efi),
            Err(DecompressError::Corrupted)
        ));
    }

    #[test]
    fn huge_orig_size_is_corrupted() {
        let mut data = vec![0u8; 16];
        data[0..4].copy_from_slice(&8u32.to_le_bytes());
        data[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            decompress(&data, Pbit::Efi),
            Err(DecompressError::Corrupted)
        ));
    }

    #[test]
    fn zero_orig_size_returns_empty() {
        let mut data = vec![0u8; 8];
        data[0..4].copy_from_slice(&8u32.to_le_bytes());
        assert_eq!(decompress(&data, Pbit::Efi).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn min_fixture_matches_oracle() {
        let out = decompress(MIN_IN, Pbit::Efi).expect("min fixture decodes");
        assert_eq!(out.len(), MIN_EXPECTED.len());
        assert_eq!(out.as_slice(), MIN_EXPECTED);
    }

    #[test]
    fn median_fixture_matches_oracle() {
        let out = decompress(MEDIAN_IN, Pbit::Efi).expect("median fixture decodes");
        assert_eq!(out.len(), MEDIAN_EXPECTED.len());
        assert_eq!(out.as_slice(), MEDIAN_EXPECTED);
    }

    #[test]
    fn max_fixture_matches_oracle() {
        let out = decompress(MAX_IN, Pbit::Efi).expect("max fixture decodes");
        assert_eq!(out.len(), MAX_EXPECTED.len());
        assert_eq!(out.as_slice(), MAX_EXPECTED);
    }
}
