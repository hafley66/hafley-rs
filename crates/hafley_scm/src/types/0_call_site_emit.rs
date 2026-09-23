/// A checked `#emit-call-site!` instruction attached to one query pattern.
pub struct CallSiteEmit {
    pub pattern: u16,
    pub group: u16,
    pub span: u16,
    pub callee_capture: Option<u16>,
    pub callee_literal: Option<u16>,
}
