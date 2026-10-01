use crate::_1_udf::{Impl, Pc, Pe, S};
use selectors::attr::{AttrSelectorOperation, CaseSensitivity, NamespaceConstraint};
use selectors::bloom::BloomFilter;
use selectors::context::MatchingContext;
use selectors::matching::ElementSelectorFlags;
use selectors::OpaqueElement;
use std::ptr::NonNull;

#[derive(Clone, Debug)]
pub struct Node<'a> {
    pub ts: tree_sitter::Node<'a>,
    pub src: &'a str,
}

impl<'a> Node<'a> {
    fn wrap(&self, ts: Option<tree_sitter::Node<'a>>) -> Option<Self> {
        ts.map(|ts| Node { ts, src: self.src })
    }

    pub fn text(&self) -> &'a str {
        &self.src[self.ts.byte_range()]
    }

    fn field(&self) -> Option<&'a str> {
        let parent = self.ts.parent()?;
        let mut cursor = parent.walk();
        let i = parent.children(&mut cursor).position(|c| c.id() == self.ts.id())?;
        parent.field_name_for_child(i as u32)
    }
}

impl<'a> selectors::Element for Node<'a> {
    type Impl = Impl;

    fn opaque(&self) -> OpaqueElement {
        // ts.id() is the subtree address inside the tree, unique per node.
        OpaqueElement::from_non_null_ptr(NonNull::new(self.ts.id() as *mut ()).unwrap())
    }
    fn parent_element(&self) -> Option<Self> {
        self.wrap(self.ts.parent())
    }
    fn parent_node_is_shadow_root(&self) -> bool {
        false
    }
    fn containing_shadow_host(&self) -> Option<Self> {
        None
    }
    fn is_pseudo_element(&self) -> bool {
        false
    }
    fn prev_sibling_element(&self) -> Option<Self> {
        self.wrap(self.ts.prev_named_sibling())
    }
    fn next_sibling_element(&self) -> Option<Self> {
        self.wrap(self.ts.next_named_sibling())
    }
    fn first_element_child(&self) -> Option<Self> {
        self.wrap(self.ts.named_child(0))
    }
    fn is_html_element_in_html_document(&self) -> bool {
        false
    }
    fn has_local_name(&self, name: &str) -> bool {
        self.ts.kind() == name
    }
    fn has_namespace(&self, _: &str) -> bool {
        true
    }
    fn is_same_type(&self, other: &Self) -> bool {
        self.ts.kind_id() == other.ts.kind_id()
    }
    fn attr_matches(
        &self,
        _: &NamespaceConstraint<&S>,
        name: &S,
        op: &AttrSelectorOperation<&S>,
    ) -> bool {
        name.0 == "field" && self.field().is_some_and(|f| op.eval_str(f))
    }
    fn match_non_ts_pseudo_class(&self, pc: &Pc, _: &mut MatchingContext<Impl>) -> bool {
        let Pc::Text(op, v) = pc;
        let t = self.text();
        match op.as_str() {
            "=" => t == v,
            "^=" => t.starts_with(v.as_str()),
            "$=" => t.ends_with(v.as_str()),
            _ => t.contains(v.as_str()),
        }
    }
    fn match_pseudo_element(&self, pe: &Pe, _: &mut MatchingContext<Impl>) -> bool {
        match *pe {}
    }
    fn apply_selector_flags(&self, _: ElementSelectorFlags) {}
    fn is_link(&self) -> bool {
        false
    }
    fn is_html_slot_element(&self) -> bool {
        false
    }
    fn has_id(&self, _: &S, _: CaseSensitivity) -> bool {
        false
    }
    fn has_class(&self, _: &S, _: CaseSensitivity) -> bool {
        false
    }
    fn has_custom_state(&self, _: &S) -> bool {
        false
    }
    fn imported_part(&self, _: &S) -> Option<S> {
        None
    }
    fn is_part(&self, _: &S) -> bool {
        false
    }
    fn is_empty(&self) -> bool {
        self.ts.named_child_count() == 0
    }
    fn is_root(&self) -> bool {
        self.ts.parent().is_none()
    }
    fn add_element_unique_hashes(&self, _: &mut BloomFilter) -> bool {
        false
    }
}
