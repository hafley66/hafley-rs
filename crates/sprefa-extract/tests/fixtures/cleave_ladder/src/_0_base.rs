#[derive(Debug, Clone)]
pub struct Base;

pub trait Show {
    fn show(&self) -> u32;
}
