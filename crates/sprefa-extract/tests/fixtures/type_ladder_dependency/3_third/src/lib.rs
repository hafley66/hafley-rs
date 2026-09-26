use ladder_collector::collector::{BulkTrigger, RowChange};

pub struct Third;

impl BulkTrigger for Third {
    fn on_batch(&mut self, _batch: &[RowChange]) {}
}
