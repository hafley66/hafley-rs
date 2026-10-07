use crate::Widget;

pub fn read(value: Widget) -> Widget {
    value
}

pub fn make() -> Widget {
    Widget { value: 0 }
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
