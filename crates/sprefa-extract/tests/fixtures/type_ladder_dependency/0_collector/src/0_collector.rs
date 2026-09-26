pub struct RowChange;

pub trait BulkTrigger {
    fn on_batch(&mut self, batch: &[RowChange]);
}
