use std::ops::Range;

/// A source slice or query-owned literal, with no borrow into the parsed tree.
pub enum EmittedValue {
    Bytes(Range<u32>),
    Literal(u16),
}

pub struct EmittedField {
    pub key: u16,
    pub value: EmittedValue,
}

pub struct EmittedFact {
    pub file: u16,
    pub relation: u16,
    pub fields: Range<u32>,
}

impl EmittedFact {
    pub fn get<'a>(&self, arena: &'a super::MatchArena, key: u16) -> Option<&'a EmittedValue> {
        arena.emitted_fields[self.fields.start as usize..self.fields.end as usize]
            .iter()
            .find(|field| field.key == key)
            .map(|field| &field.value)
    }
}

impl EmittedValue {
    pub fn bytes(&self) -> Option<&Range<u32>> {
        match self {
            Self::Bytes(bytes) => Some(bytes),
            Self::Literal(_) => None,
        }
    }

    pub fn text<'a>(&self, src: &'a [u8], query: &'a super::QueryExt) -> Option<&'a str> {
        match self {
            Self::Bytes(bytes) => std::str::from_utf8(&src[bytes.start as usize..bytes.end as usize]).ok(),
            Self::Literal(index) => Some(&query.emit_literals[*index as usize]),
        }
    }
}
