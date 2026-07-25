pub const FFS_ALIGN: usize = 8;
pub const SECTION_ALIGN: usize = 4;

pub fn align_up(offset: usize, align: usize) -> usize {
    (offset + align - 1) & !(align - 1)
}

pub fn align8(offset: usize) -> usize {
    align_up(offset, FFS_ALIGN)
}

pub fn align4(offset: usize) -> usize {
    align_up(offset, SECTION_ALIGN)
}

pub fn pad_to(buf: &mut Vec<u8>, target: usize, fill: u8) {
    if buf.len() < target {
        buf.resize(target, fill);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_up_rounds_to_boundary() {
        assert_eq!(align8(0), 0);
        assert_eq!(align8(1), 8);
        assert_eq!(align8(8), 8);
        assert_eq!(align8(9), 16);
        assert_eq!(align4(3), 4);
        assert_eq!(align4(4), 4);
    }

    #[test]
    fn pad_to_grows_with_fill() {
        let mut buf = vec![0x01u8, 0x02];
        pad_to(&mut buf, 4, 0xFF);
        assert_eq!(buf, vec![0x01, 0x02, 0xFF, 0xFF]);
        pad_to(&mut buf, 2, 0x00);
        assert_eq!(buf.len(), 4);
    }
}
