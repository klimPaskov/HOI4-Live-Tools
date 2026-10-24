//! Bounded PE32+ x86_64 header identity, not a layout or RTTI decoder.
use super::{error, Result, HEADER_BOUND};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PeIdentity {
    pub timestamp: u32,
    pub image_size: u32,
    pub header_size: u32,
    pub preferred_base: u64,
    pub sections: u16,
}
fn bytes<const N: usize>(data: &[u8], offset: usize) -> Result<[u8; N]> {
    data.get(
        offset
            ..offset
                .checked_add(N)
                .ok_or_else(|| error("invalid_pe", "Header arithmetic overflow"))?,
    )
    .and_then(|v| v.try_into().ok())
    .ok_or_else(|| error("invalid_pe", "Truncated PE header"))
}
impl PeIdentity {
    pub(crate) fn parse(data: &[u8]) -> Result<Self> {
        if data.len() > HEADER_BOUND || data.get(..2) != Some(b"MZ") {
            return Err(error("invalid_pe", "Bounded MZ header required"));
        }
        let pe = u32::from_le_bytes(bytes(data, 0x3c)?) as usize;
        if !(0x40..=HEADER_BOUND - 264).contains(&pe) || bytes::<4>(data, pe)? != *b"PE\0\0" {
            return Err(error("invalid_pe", "Invalid PE signature offset"));
        }
        let machine = u16::from_le_bytes(bytes(data, pe + 4)?);
        let sections = u16::from_le_bytes(bytes(data, pe + 6)?);
        let timestamp = u32::from_le_bytes(bytes(data, pe + 8)?);
        let optional = u16::from_le_bytes(bytes(data, pe + 20)?) as usize;
        let characteristics = u16::from_le_bytes(bytes(data, pe + 22)?);
        if machine != 0x8664
            || !(1..=96).contains(&sections)
            || !(240..=4096).contains(&optional)
            || characteristics & 0x2 == 0
            || characteristics & 0x2000 != 0
        {
            return Err(error(
                "unsupported_build",
                "Only bounded executable x86_64 PE32+ is supported",
            ));
        }
        let o = pe + 24;
        if u16::from_le_bytes(bytes(data, o)?) != 0x20b {
            return Err(error("unsupported_build", "PE32+ required"));
        }
        let preferred_base = u64::from_le_bytes(bytes(data, o + 24)?);
        let section_align = u32::from_le_bytes(bytes(data, o + 32)?);
        let file_align = u32::from_le_bytes(bytes(data, o + 36)?);
        let image_size = u32::from_le_bytes(bytes(data, o + 56)?);
        let header_size = u32::from_le_bytes(bytes(data, o + 60)?);
        let table = o
            .checked_add(optional)
            .and_then(|v| v.checked_add(usize::from(sections) * 40))
            .ok_or_else(|| error("invalid_pe", "PE section table overflow"))?;
        if preferred_base < 0x10000
            || preferred_base % 65536 != 0
            || image_size == 0
            || image_size > 1 << 30
            || !(512..=HEADER_BOUND as u32).contains(&header_size)
            || header_size as usize > data.len()
            || table > header_size as usize
            || header_size > image_size
            || !section_align.is_power_of_two()
            || section_align < 4096
            || !file_align.is_power_of_two()
            || !(512..=65536).contains(&file_align)
            || file_align > section_align
            || image_size % section_align != 0
            || header_size % file_align != 0
        {
            return Err(error(
                "invalid_pe",
                "Unqualified PE image/header dimensions",
            ));
        }
        let mut end = header_size;
        for n in 0..usize::from(sections) {
            let a = o + optional + n * 40;
            let virtual_size = u32::from_le_bytes(bytes(data, a + 8)?);
            let rva = u32::from_le_bytes(bytes(data, a + 12)?);
            let raw_size = u32::from_le_bytes(bytes(data, a + 16)?);
            if rva < section_align || rva % section_align != 0 || rva < end || virtual_size == 0 {
                return Err(error("invalid_pe", "Invalid ordered PE sections"));
            }
            end = rva
                .checked_add(virtual_size.max(raw_size))
                .filter(|v| *v <= image_size)
                .ok_or_else(|| error("invalid_pe", "PE section exceeds image"))?;
        }
        Ok(Self {
            timestamp,
            image_size,
            header_size,
            preferred_base,
            sections,
        })
    }
}
