//! Known Sims 3 resource type ids.

pub const NMAP: u32 = 0x0166038C;
pub const MODL: u32 = 0x01661233;
pub const MLOD: u32 = 0x01D10F34;
pub const GEOM: u32 = 0x015A1849;
pub const DDS: u32 = 0x00B2D882;
pub const PNG: u32 = 0x2F7D0004;
pub const OBJD: u32 = 0x319E4F1D;
pub const OBJK: u32 = 0x02DC343F;
pub const CASP: u32 = 0x034AEECB;
pub const STBL: u32 = 0x220557DA;
pub const RIG: u32 = 0x8EAF13DE;
pub const CLIP: u32 = 0x6B20C4F3;
pub const S3SA: u32 = 0x073FAA07;
pub const LITE: u32 = 0x03B4C61D;
pub const FTPT: u32 = 0xD382BF57;
pub const SIMO: u32 = 0x025ED6F4;
pub const TXTC: u32 = 0x033A1435;
pub const VPXY: u32 = 0x736884F1;

pub fn name(t: u32) -> &'static str {
    match t {
        NMAP => "NMAP",
        MODL => "MODL",
        MLOD => "MLOD",
        GEOM => "GEOM",
        DDS => "DDS",
        PNG => "PNG",
        OBJD => "OBJD",
        OBJK => "OBJK",
        CASP => "CASP",
        STBL => "STBL",
        RIG => "RIG",
        CLIP => "CLIP",
        S3SA => "S3SA",
        LITE => "LITE",
        FTPT => "FTPT",
        SIMO => "SIMO",
        TXTC => "TXTC",
        VPXY => "VPXY",
        0x0333406C => "XML",
        0x02B9F662 => "PROP",
        0x0355E0A6 => "BOND",
        0x0358B08A => "FACE",
        0xD4D9FBE5 => "PTRN",
        0x025C95B6 => "LAYO",
        0x01D0E75D => "MATD",
        0x01D0E6FB => "VBUF",
        0x01D0E70F => "IBUF",
        0x01D0E723 => "VRTF",
        0x02019972 => "MTST",
        0xD3044521 => "RSLT",
        0x0418FE2A => "CFEN",
        0x049CA4CD => "CSTR",
        0x04A4D951 => "CPRX",
        0x04F3CC01 => "CFIR",
        0x0A36F07A => "CCFP",
        0x316C78F2 => "CFND",
        0x9151E6BC => "CWST",
        0x9A20CD1C => "CSTS?",
        0xA8F7B517 => "CWNS",
        0x515CA4CD => "CWAL",
        0x0D338A3A => "THUM?",
        0x2E75C764 => "ICON",
        0x2E75C765 => "ICON2",
        0x2E75C766 => "ICON3",
        0x2E75C767 => "ICON4",
        0x626F60CE => "THUM",
        0x0580A2B4 => "THUM",
        0x0580A2B5 => "THUM",
        0x0580A2B6 => "THUM",
        _ => "?",
    }
}
