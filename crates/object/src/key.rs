#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum PropertyKey {
    Index(u32),
    String(String),
    Symbol(u32),
}

impl PropertyKey {
    pub fn from_string(value: impl Into<String>) -> Self {
        let value = value.into();
        if let Ok(index) = value.parse::<u32>()
            && index.to_string() == value
        {
            return Self::Index(index);
        }
        Self::String(value)
    }

    pub const fn as_index(&self) -> Option<u32> {
        match self {
            Self::Index(index) => Some(*index),
            _ => None,
        }
    }
}

pub fn ordered_keys(keys: &[PropertyKey]) -> Vec<PropertyKey> {
    let mut indices = Vec::new();
    let mut others = Vec::new();
    for key in keys {
        if let PropertyKey::Index(index) = key {
            indices.push((*index, key.clone()));
        } else {
            others.push(key.clone());
        }
    }
    indices.sort_by_key(|(index, _)| *index);
    indices
        .into_iter()
        .map(|(_, key)| key)
        .chain(others)
        .collect()
}
