use crate::types::Guid;

pub use r_efi::hii::IFR_ACTION_OP as OP_ACTION;
pub use r_efi::hii::IFR_CHECKBOX_OP as OP_CHECKBOX;
pub use r_efi::hii::IFR_DEFAULT_OP as OP_DEFAULT;
pub use r_efi::hii::IFR_DEFAULTSTORE_OP as OP_DEFAULT_STORE;
pub use r_efi::hii::IFR_END_OP as OP_END;
pub use r_efi::hii::IFR_FORM_OP as OP_FORM;
pub use r_efi::hii::IFR_FORM_SET_OP as OP_FORM_SET;
pub use r_efi::hii::IFR_NUMERIC_OP as OP_NUMERIC;
pub use r_efi::hii::IFR_ONE_OF_OP as OP_ONE_OF;
pub use r_efi::hii::IFR_ONE_OF_OPTION_OP as OP_ONE_OF_OPTION;
pub use r_efi::hii::IFR_ORDERED_LIST_OP as OP_ORDERED_LIST;
pub use r_efi::hii::IFR_REF_OP as OP_REF;
pub use r_efi::hii::IFR_STRING_OP as OP_STRING;
pub use r_efi::hii::IFR_TEXT_OP as OP_TEXT;
pub use r_efi::hii::IFR_VARSTORE_EFI_OP as OP_VARSTORE_EFI;
pub use r_efi::hii::IFR_VARSTORE_OP as OP_VARSTORE;

pub use r_efi::hii::IFR_TYPE_BOOLEAN as TYPE_BOOLEAN;
pub use r_efi::hii::IFR_TYPE_NUM_SIZE_8 as TYPE_NUM_SIZE_8;
pub use r_efi::hii::IFR_TYPE_NUM_SIZE_16 as TYPE_NUM_SIZE_16;
pub use r_efi::hii::IFR_TYPE_NUM_SIZE_32 as TYPE_NUM_SIZE_32;
pub use r_efi::hii::IFR_TYPE_NUM_SIZE_64 as TYPE_NUM_SIZE_64;
pub use r_efi::hii::IFR_TYPE_STRING as TYPE_STRING;

pub use r_efi::hii::IFR_CHECKBOX_DEFAULT;
pub use r_efi::hii::IFR_CHECKBOX_DEFAULT_MFG;
pub use r_efi::hii::IFR_DISPLAY_INT_DEC;
pub use r_efi::hii::IFR_DISPLAY_UINT_DEC;
pub use r_efi::hii::IFR_DISPLAY_UINT_HEX;
pub use r_efi::hii::IFR_OPTION_DEFAULT;
pub use r_efi::hii::IFR_OPTION_DEFAULT_MFG;

pub const DEFAULT_ID_STANDARD: u16 = 0x0000;
pub const DEFAULT_ID_MANUFACTURING: u16 = 0x0001;

pub struct IfrBuilder {
    buf: Vec<u8>,
}

impl Default for IfrBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl IfrBuilder {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn write_header(&mut self, opcode: u8, scope: bool, data_len: usize) {
        let total = (data_len + 2) as u8;
        let length = total & 0x7F;
        let scope_bit: u8 = if scope { 0x80 } else { 0x00 };
        self.buf.push(opcode);
        self.buf.push(length | scope_bit);
    }

    pub fn emit_form_set(
        &mut self,
        guid: &Guid,
        title_id: u16,
        help_id: u16,
        class_guids: &[Guid],
    ) {
        let flags = class_guids.len() as u8 & 0x03;
        self.write_header(OP_FORM_SET, true, 21 + 16 * class_guids.len());
        self.buf.extend_from_slice(&guid_to_bytes(guid));
        self.buf.extend_from_slice(&title_id.to_le_bytes());
        self.buf.extend_from_slice(&help_id.to_le_bytes());
        self.buf.push(flags);
        for cg in class_guids {
            self.buf.extend_from_slice(&guid_to_bytes(cg));
        }
    }

    pub fn emit_var_store(&mut self, id: u16, guid: &Guid, size: u16, name: &str) {
        let name_bytes = name.as_bytes();
        self.write_header(OP_VARSTORE, false, 20 + name_bytes.len() + 1);
        self.buf.extend_from_slice(&guid_to_bytes(guid));
        self.buf.extend_from_slice(&id.to_le_bytes());
        self.buf.extend_from_slice(&size.to_le_bytes());
        self.buf.extend_from_slice(name_bytes);
        self.buf.push(0);
    }

    /// IfrVarStoreEfi (EDK2): VarStoreId@2, Guid@4, Attributes@20,
    /// Size@24, Name@26 UCS-2+NUL. Зеркально values::varstore_map.
    pub fn emit_var_store_efi(
        &mut self,
        id: u16,
        guid: &Guid,
        size: u16,
        name: &str,
        attributes: u32,
    ) {
        let name_bytes: Vec<u8> = name
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .chain([0, 0])
            .collect();
        self.write_header(OP_VARSTORE_EFI, false, 24 + name_bytes.len());
        self.buf.extend_from_slice(&id.to_le_bytes());
        self.buf.extend_from_slice(&guid_to_bytes(guid));
        self.buf.extend_from_slice(&attributes.to_le_bytes());
        self.buf.extend_from_slice(&size.to_le_bytes());
        self.buf.extend_from_slice(&name_bytes);
    }

    pub fn emit_default_store(&mut self, name_id: u16, default_id: u16) {
        self.write_header(OP_DEFAULT_STORE, false, 4);
        self.buf.extend_from_slice(&name_id.to_le_bytes());
        self.buf.extend_from_slice(&default_id.to_le_bytes());
    }

    pub fn emit_form(&mut self, id: u16, title_id: u16) {
        self.write_header(OP_FORM, true, 4);
        self.buf.extend_from_slice(&id.to_le_bytes());
        self.buf.extend_from_slice(&title_id.to_le_bytes());
    }

    #[allow(clippy::too_many_arguments)]
    pub fn emit_one_of(
        &mut self,
        prompt_id: u16,
        help_id: u16,
        qid: u16,
        vsid: u16,
        voff: u16,
        flags: u8,
        size: u8,
    ) {
        self.write_header(OP_ONE_OF, true, 12 + 3 * size as usize);
        self.buf.extend_from_slice(&prompt_id.to_le_bytes());
        self.buf.extend_from_slice(&help_id.to_le_bytes());
        self.buf.extend_from_slice(&qid.to_le_bytes());
        self.buf.extend_from_slice(&vsid.to_le_bytes());
        self.buf.extend_from_slice(&voff.to_le_bytes());
        self.buf.push(0);
        let numeric_flags = (size - 1) & 0x03;
        self.buf.push(numeric_flags | flags);
        let (min, max, step) = min_max_step_default(size);
        self.buf.extend_from_slice(&min);
        self.buf.extend_from_slice(&max);
        self.buf.extend_from_slice(&step);
    }

    pub fn emit_one_of_option(
        &mut self,
        text_id: u16,
        flags: u8,
        value_type: u8,
        value: u64,
        size: u8,
    ) {
        let val_bytes = value_to_bytes(value, size);
        self.write_header(OP_ONE_OF_OPTION, false, 4 + val_bytes.len());
        self.buf.extend_from_slice(&text_id.to_le_bytes());
        self.buf.push(flags | value_type);
        self.buf.push(value_type);
        self.buf.extend_from_slice(&val_bytes);
    }

    pub fn emit_check_box(
        &mut self,
        prompt_id: u16,
        help_id: u16,
        qid: u16,
        vsid: u16,
        voff: u16,
        flags: u8,
    ) {
        self.write_header(OP_CHECKBOX, true, 12);
        self.buf.extend_from_slice(&prompt_id.to_le_bytes());
        self.buf.extend_from_slice(&help_id.to_le_bytes());
        self.buf.extend_from_slice(&qid.to_le_bytes());
        self.buf.extend_from_slice(&vsid.to_le_bytes());
        self.buf.extend_from_slice(&voff.to_le_bytes());
        self.buf.push(0);
        self.buf.push(flags);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn emit_numeric(
        &mut self,
        prompt_id: u16,
        help_id: u16,
        qid: u16,
        vsid: u16,
        voff: u16,
        flags: u8,
        size: u8,
        min: u64,
        max: u64,
        step: u64,
    ) {
        self.write_header(OP_NUMERIC, true, 12 + 3 * size as usize);
        self.buf.extend_from_slice(&prompt_id.to_le_bytes());
        self.buf.extend_from_slice(&help_id.to_le_bytes());
        self.buf.extend_from_slice(&qid.to_le_bytes());
        self.buf.extend_from_slice(&vsid.to_le_bytes());
        self.buf.extend_from_slice(&voff.to_le_bytes());
        self.buf.push(0);
        let numeric_flags = (size - 1) & 0x03;
        self.buf.push(numeric_flags | flags);
        self.buf.extend_from_slice(&value_to_bytes(min, size));
        self.buf.extend_from_slice(&value_to_bytes(max, size));
        self.buf.extend_from_slice(&value_to_bytes(step, size));
    }

    pub fn emit_ref(
        &mut self,
        prompt_id: u16,
        help_id: u16,
        qid: u16,
        vsid: u16,
        voff: u16,
        form_id: u16,
    ) {
        self.write_header(OP_REF, false, 13);
        self.buf.extend_from_slice(&prompt_id.to_le_bytes());
        self.buf.extend_from_slice(&help_id.to_le_bytes());
        self.buf.extend_from_slice(&qid.to_le_bytes());
        self.buf.extend_from_slice(&vsid.to_le_bytes());
        self.buf.extend_from_slice(&voff.to_le_bytes());
        self.buf.push(0);
        self.buf.extend_from_slice(&form_id.to_le_bytes());
    }

    /// REF3 (спека formset-unlock §2): кросс-формсетный GOTO, total 33.
    /// QuestionId — 0xFFFF (EFI_QUESTION_ID_INVALID, паттерн EDK2 CIfrRef3).
    pub fn emit_ref3(
        &mut self,
        prompt_id: u16,
        help_id: u16,
        qid: u16,
        form_id: u16,
        formset: &Guid,
    ) {
        self.write_header(OP_REF, false, 31);
        self.buf.extend_from_slice(&prompt_id.to_le_bytes());
        self.buf.extend_from_slice(&help_id.to_le_bytes());
        self.buf.extend_from_slice(&qid.to_le_bytes());
        self.buf.extend_from_slice(&0xFFFFu16.to_le_bytes());
        self.buf.extend_from_slice(&0xFFFFu16.to_le_bytes());
        self.buf.push(0);
        self.buf.extend_from_slice(&form_id.to_le_bytes());
        self.buf.extend_from_slice(&0xFFFFu16.to_le_bytes());
        self.buf.extend_from_slice(&guid_to_bytes(formset));
    }

    pub fn emit_text(&mut self, prompt_id: u16, help_id: u16, text_two_id: u16) {
        self.write_header(OP_TEXT, false, 6);
        self.buf.extend_from_slice(&prompt_id.to_le_bytes());
        self.buf.extend_from_slice(&help_id.to_le_bytes());
        self.buf.extend_from_slice(&text_two_id.to_le_bytes());
    }

    pub fn emit_default(&mut self, default_id: u16, value_type: u8, value: u64, size: u8) {
        let val_bytes = value_to_bytes(value, size);
        self.write_header(OP_DEFAULT, false, 3 + val_bytes.len());
        self.buf.extend_from_slice(&default_id.to_le_bytes());
        self.buf.push(value_type);
        self.buf.extend_from_slice(&val_bytes);
    }

    pub fn emit_end(&mut self) {
        self.write_header(OP_END, false, 0);
    }

    pub fn build(self) -> Vec<u8> {
        self.buf
    }
}

pub fn guid_to_bytes(g: &Guid) -> [u8; 16] {
    g.to_bytes()
}

fn value_to_bytes(v: u64, size: u8) -> Vec<u8> {
    match size {
        1 => vec![v as u8],
        2 => (v as u16).to_le_bytes().to_vec(),
        4 => (v as u32).to_le_bytes().to_vec(),
        8 => v.to_le_bytes().to_vec(),
        _ => vec![v as u8],
    }
}

fn min_max_step_default(size: u8) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let z = value_to_bytes(0, size);
    let m = value_to_bytes(
        match size {
            1 => 0xFF,
            2 => 0xFFFF,
            4 => 0xFFFFFFFF,
            _ => 0xFFFFFFFFFFFFFFFF,
        },
        size,
    );
    (z.clone(), m, z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn emit_form_set_header() {
        let mut b = IfrBuilder::new();
        let g = Guid::from_str("A1B2C3D4-E5F6-7890-ABCD-EF1234567890").unwrap();
        b.emit_form_set(&g, 1, 2, &[]);
        let buf = b.build();
        assert_eq!(buf[0], OP_FORM_SET);
        assert_eq!(buf[1] & 0x7F, 23);
        assert_eq!(buf[1] & 0x80, 0x80);
        assert_eq!(&buf[2..18], &guid_to_bytes(&g));
        assert_eq!(u16::from_le_bytes([buf[18], buf[19]]), 1);
    }

    #[test]
    fn emit_form_header() {
        let mut b = IfrBuilder::new();
        b.emit_form(5, 10);
        let buf = b.build();
        assert_eq!(buf[0], OP_FORM);
        assert_eq!(buf[1] & 0x80, 0x80);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 5);
        assert_eq!(u16::from_le_bytes([buf[4], buf[5]]), 10);
    }

    #[test]
    fn emit_end() {
        let mut b = IfrBuilder::new();
        b.emit_end();
        let buf = b.build();
        assert_eq!(buf, vec![OP_END, 0x02]);
    }

    #[test]
    fn emit_var_store_efi_layout() {
        let mut b = IfrBuilder::new();
        let g = Guid::from_str("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9").unwrap();
        b.emit_var_store_efi(0x7F01, &g, 0x1670, "IntelSetup", 7);
        let buf = b.build();
        assert_eq!(buf[0], OP_VARSTORE_EFI);
        // total = 2 header + id(2) + guid(16) + attr(4) + size(2) + имя
        // UCS-2 (10 симв. × 2 + NUL 2) = 48
        assert_eq!(buf[1] & 0x7F, 48);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 0x7F01);
        assert_eq!(&buf[4..20], &g.to_bytes());
        assert_eq!(u32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]), 7);
        assert_eq!(u16::from_le_bytes([buf[24], buf[25]]), 0x1670);
        assert_eq!(
            &buf[26..48],
            "I\0n\0t\0e\0l\0S\0e\0t\0u\0p\0\0\0".as_bytes()
        );
    }

    #[test]
    fn emit_var_store_with_name() {
        let mut b = IfrBuilder::new();
        let g = Guid::from_str("11111111-2222-3333-4444-555555555555").unwrap();
        b.emit_var_store(1, &g, 256, "MyVar");
        let buf = b.build();
        assert_eq!(buf[0], OP_VARSTORE);
        assert_eq!(u16::from_le_bytes([buf[18], buf[19]]), 1);
        assert_eq!(u16::from_le_bytes([buf[20], buf[21]]), 256);
        assert_eq!(&buf[22..27], b"MyVar");
        assert_eq!(buf[27], 0);
    }

    #[test]
    fn emit_one_of_option_u8() {
        let mut b = IfrBuilder::new();
        b.emit_one_of_option(5, 0x10, TYPE_NUM_SIZE_8, 2, 1);
        let buf = b.build();
        assert_eq!(buf[0], OP_ONE_OF_OPTION);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), 5);
        assert_eq!(buf[6], 2);
    }

    #[test]
    fn emit_ref3_layout() {
        let mut b = IfrBuilder::new();
        let g = Guid::from_str("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9").unwrap();
        b.emit_ref3(0x40, 0x41, 0x7F10, 1, &g);
        let buf = b.build();
        assert_eq!(buf[0], OP_REF);
        assert_eq!(buf[1] & 0x7F, 33);
        assert_eq!(u16::from_le_bytes([buf[13], buf[14]]), 1);
        assert_eq!(u16::from_le_bytes([buf[15], buf[16]]), 0xFFFF);
        assert_eq!(&buf[17..33], &g.to_bytes());
    }

    #[test]
    fn emit_default_standard() {
        let mut b = IfrBuilder::new();
        b.emit_default(DEFAULT_ID_STANDARD, TYPE_NUM_SIZE_8, 1, 1);
        let buf = b.build();
        assert_eq!(buf[0], OP_DEFAULT);
        assert_eq!(u16::from_le_bytes([buf[2], buf[3]]), DEFAULT_ID_STANDARD);
        assert_eq!(buf[4], TYPE_NUM_SIZE_8);
        assert_eq!(buf[5], 1);
    }
}
