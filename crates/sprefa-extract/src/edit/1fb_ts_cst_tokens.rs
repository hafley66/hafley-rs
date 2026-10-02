//! Identifier tokens a checker is asked about, read off the CST plane: the name
//! a call site invokes and the name of each JSX attribute.

use std::collections::HashMap;

use crate::{FamilyTag, FlatFact};

struct CstNode {
    start: u32,
    end: u32,
    leaf: bool,
    attribute: bool,
}

/// One file's named CST nodes in the extraction's preorder, linked into a tree.
#[derive(Default)]
pub struct CstTokens {
    nodes: Vec<CstNode>,
    children: Vec<Vec<usize>>,
    by_span: HashMap<(u32, u32), usize>,
}

impl CstTokens {
    /// One raw fact; only CST nodes are kept, in arrival order.
    pub fn push(&mut self, fact: &FlatFact) {
        if let FlatFact::Node { family: FamilyTag::Cst, span, kind, name, .. } = fact {
            // A leaf identifier's CST name is its own text.
            let leaf = name.as_ref().is_some_and(|name| name.len() as u32 == span.end - span.start);
            self.nodes.push(CstNode {
                start: span.start,
                end: span.end,
                leaf,
                attribute: kind == "jsx_attribute",
            });
        }
    }

    /// Link each node under the innermost earlier node containing it.
    pub fn seal(&mut self) {
        self.children = vec![Vec::new(); self.nodes.len()];
        let mut open: Vec<usize> = Vec::new();
        for index in 0..self.nodes.len() {
            let (start, end) = (self.nodes[index].start, self.nodes[index].end);
            while open.last().is_some_and(|&top| {
                let top = &self.nodes[top];
                !(top.start <= start && end <= top.end)
            }) {
                open.pop();
            }
            if let Some(&parent) = open.last() {
                self.children[parent].push(index);
            }
            open.push(index);
            // Preorder: a later node on the same span is the deeper one.
            self.by_span.insert((start, end), index);
        }
    }

    /// The `name` token the call site spanning `[start, end)` invokes: the site
    /// node's own leaf, its last child leaf so named, else down its first child.
    pub fn callee(&self, text: &str, start: u32, end: u32, name: &str) -> Option<(u32, u32)> {
        let named = |index: usize| {
            let node = &self.nodes[index];
            node.leaf && text.get(node.start as usize..node.end as usize) == Some(name)
        };
        let mut node = *self.by_span.get(&(start, end))?;
        loop {
            let _step = tracing::trace_span!("ts.callee.step").entered();
            let kids = &self.children[node];
            if let Some(leaf) = std::iter::once(node).chain(kids.iter().rev().copied()).find(|&index| named(index)) {
                return Some((self.nodes[leaf].start, self.nodes[leaf].end));
            }
            node = *kids.first()?;
        }
    }

    /// Each JSX attribute's name token: the attribute node's first child.
    pub fn attribute_names(&self) -> impl Iterator<Item = (u32, u32)> + '_ {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.attribute)
            .filter_map(|(index, _)| self.children[index].first())
            .map(|&name| (self.nodes[name].start, self.nodes[name].end))
    }
}
