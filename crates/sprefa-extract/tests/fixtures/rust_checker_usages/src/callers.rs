use crate::widget::render as draw;

macro_rules! paint {
    () => {
        crate::widget::render()
    };
}

pub fn plain() -> u32 {
    crate::widget::render()
}

pub fn aliased() -> u32 {
    draw()
}

pub fn expanded() -> u32 {
    paint!()
}

pub fn decoyed() -> u32 {
    crate::decoy::render()
}
