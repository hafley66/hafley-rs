use crate::Widget;

pub fn read(value: Widget) -> Widget {
    value
}

pub struct Reader;

impl Reader {
    pub fn method(&self, value: Widget) -> Widget {
        value
    }
}

pub trait Reads {
    fn default(&self, value: Widget) -> Widget {
        value
    }
}
