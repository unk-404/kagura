#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElementsKind {
    Smi,
    Double,
    Object,
    HoleySmi,
    HoleyDouble,
    HoleyObject,
    Dictionary,
    Cow,
}

impl ElementsKind {
    pub const fn is_holey(self) -> bool {
        matches!(self, Self::HoleySmi | Self::HoleyDouble | Self::HoleyObject)
    }
}
