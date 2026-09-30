pub struct Lines(pub Vec<u32>);

pub fn count(lines: &Lines) -> u32 {
    lines.0.len() as u32
}

pub fn total() -> u32 {
    count(&Lines(vec![1, 2]))
}
