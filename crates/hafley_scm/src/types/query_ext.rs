/// The user's `.scm` compiled once per language. Built once, run over many files.
pub struct QueryExt {
    pub user: tree_sitter::Query,
    pub names: Vec<Box<str>>,
    pub emits: Vec<super::EmitSpec>,
    pub relations: Vec<Box<str>>,
    pub fields: Vec<Box<str>>,
    pub emit_literals: Vec<Box<str>>,
}

impl QueryExt {
    pub fn relation_id(&self, name: &str) -> Option<u16> {
        self.relations
            .iter()
            .position(|value| value.as_ref() == name)
            .map(|index| index as u16)
    }

    pub fn field_id(&self, name: &str) -> Option<u16> {
        self.fields
            .iter()
            .position(|value| value.as_ref() == name)
            .map(|index| index as u16)
    }
}
