pub struct Local {
    items: Vec<u32>,
}

fn len(items: &[u32]) -> usize {
    items.len()
}

impl Local {
    pub fn len(&self) -> usize {
        len(&self.items)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
