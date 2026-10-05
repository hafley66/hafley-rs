pub fn objects() {}
pub fn local() { objects(); }
pub struct Worker;
impl Worker {
    pub fn method(&self) {}
    pub fn run(&self) { self.method(); }
}
#[path = "1_child.rs"]
mod child;
pub fn nested() { child::child(); }
