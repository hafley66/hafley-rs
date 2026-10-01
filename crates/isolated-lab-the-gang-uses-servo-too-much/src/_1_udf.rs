use cssparser::{CowRcStr, ParseError, ToCss, Token};
use precomputed_hash::PrecomputedHash;
use selectors::parser::{NonTSPseudoClass, PseudoElement, SelectorParseErrorKind};
use std::borrow::Borrow;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct S(pub String);

impl From<&str> for S {
    fn from(s: &str) -> Self {
        S(s.to_string())
    }
}
impl AsRef<str> for S {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
impl Borrow<str> for S {
    fn borrow(&self) -> &str {
        &self.0
    }
}
impl PrecomputedHash for S {
    fn precomputed_hash(&self) -> u32 {
        self.0.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32))
    }
}
impl ToCss for S {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        dest.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pc {
    Text(String, String),
}

impl ToCss for Pc {
    fn to_css<W: fmt::Write>(&self, dest: &mut W) -> fmt::Result {
        let Pc::Text(op, v) = self;
        write!(dest, ":text({op} {v:?})")
    }
}
impl NonTSPseudoClass for Pc {
    fn is_active_or_hover(&self) -> bool {
        false
    }
    fn is_user_action_state(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pe {}

impl ToCss for Pe {
    fn to_css<W: fmt::Write>(&self, _: &mut W) -> fmt::Result {
        match *self {}
    }
}
impl PseudoElement for Pe {}

#[derive(Debug, Clone)]
pub struct Impl;

impl selectors::SelectorImpl for Impl {
    type ExtraMatchingData<'a> = ();
    type AttrValue = S;
    type Identifier = S;
    type LocalName = S;
    type NamespaceUrl = S;
    type NamespacePrefix = S;
    type BorrowedNamespaceUrl = str;
    type BorrowedLocalName = str;
    type NonTSPseudoClass = Pc;
    type PseudoElement = Pe;
}

pub struct P;

impl<'i> selectors::Parser<'i> for P {
    type Impl = Impl;
    type Error = SelectorParseErrorKind;

    fn parse_has(&self) -> bool {
        true
    }
    fn parse_is_and_where(&self) -> bool {
        true
    }

    fn parse_non_ts_functional_pseudo_class(
        &self,
        name: CowRcStr<'i>,
        input: &mut cssparser::Parser<'i>,
        _after_part: bool,
    ) -> Result<Pc, ParseError<Self::Error>> {
        if &*name != "text" {
            return Err(ParseError::custom(SelectorParseErrorKind::UnsupportedPseudoClassOrElement));
        }
        let op = match input.next()? {
            Token::Delim('=') => "=",
            Token::PrefixMatch => "^=",
            Token::SuffixMatch => "$=",
            Token::SubstringMatch => "*=",
            _ => return Err(ParseError::unexpected_token()),
        };
        let v = input.expect_string()?.to_string();
        Ok(Pc::Text(op.to_string(), v))
    }
}
