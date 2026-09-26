use crate::Req4;

pub fn start(req: &Req4) -> u32 {
    req.span.at
}

#[cfg(test)]
mod tests {
    use crate::_4_types::{Req4, Span4};

    #[test]
    fn starts_at_zero() {
        let req = Req4 { span: Span4 { at: 0 } };
        assert_eq!(super::start(&req), 0);
    }
}
