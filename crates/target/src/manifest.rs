#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PtrWidth {
    W32,
    W64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Endian {
    Little,
    Big,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueRepr {
    Tagged32,
    NanBox64,
}

#[derive(Clone, Copy, Debug)]
pub struct TargetManifest {
    pub ptr_width: PtrWidth,
    pub endian: Endian,
    pub value_repr: ValueRepr,
    pub page_size: u32,
    pub atomic_width_max: u8,
}

impl TargetManifest {
    pub fn bootstrap() -> Self {
        #[cfg(target_pointer_width = "32")]
        const PTR_WIDTH: PtrWidth = PtrWidth::W32;
        #[cfg(target_pointer_width = "64")]
        const PTR_WIDTH: PtrWidth = PtrWidth::W64;

        #[cfg(target_endian = "little")]
        const ENDIAN: Endian = Endian::Little;
        #[cfg(target_endian = "big")]
        const ENDIAN: Endian = Endian::Big;

        Self {
            ptr_width: PTR_WIDTH,
            endian: ENDIAN,
            value_repr: ValueRepr::Tagged32,
            page_size: 4096,
            atomic_width_max: 32,
        }
    }
}
