use crate::read::family::{CstEdgeKind, CstF};
use crate::read::lang::call_kinds::{CALLEE_NAME_KINDS, NAME_LEAF_KINDS};
use crate::read::lang::extract_lang::RyiLang;
use crate::read::rows::{Edge, FamilyBundle, Node};
use crate::read::shape::{NodeRef, Span, Strings};

struct NameKinds(Vec<u16>);

impl NameKinds {
    fn resolve(lang: &RyiLang) -> Self {
        Self(
            NAME_LEAF_KINDS
                .iter()
                .map(|kind| lang.kind_to_id(kind))
                .filter(|id| *id != 0)
                .collect(),
        )
    }

    fn contains(&self, id: u16) -> bool {
        self.0.contains(&id)
    }
}

pub fn cst_bundle(path: &str, content: &[u8], strings: &mut Strings) -> Option<FamilyBundle<CstF>> {
    let lang = RyiLang::from_path(path)?;
    let src = std::str::from_utf8(content).ok()?;
    let language = lang.tree_sitter_language();
    let tree = hafley_scm::cst::parse(&language, content)?;
    cst_bundle_from_tree_for_lang(&lang, src, &tree, strings)
}

pub fn cst_bundle_from_tree(
    path: &str,
    content: &[u8],
    tree: &tree_sitter::Tree,
    strings: &mut Strings,
) -> Option<FamilyBundle<CstF>> {
    let lang = RyiLang::from_path(path)?;
    let src = std::str::from_utf8(content).ok()?;
    cst_bundle_from_tree_for_lang(&lang, src, tree, strings)
}

fn cst_bundle_from_tree_for_lang(
    lang: &RyiLang,
    src: &str,
    tree: &tree_sitter::Tree,
    strings: &mut Strings,
) -> Option<FamilyBundle<CstF>> {
    let language = lang.tree_sitter_language();
    let kinds = NameKinds::resolve(lang);
    let mut bundle = FamilyBundle::<CstF>::default();
    hafley_scm::cst::walk_named_streaming(
        tree,
        |ix, parent, kind_id, start, end, name, named_children| {
            let kind_text = hafley_scm::cst::kind_name(&language, kind_id);
            let mut node = Node::new(
                Span {
                    start,
                    len: end - start,
                },
                strings.intern(kind_text),
            );
            node.name = match name {
                Some((name_start, name_end)) => {
                    Some(strings.intern(&src[name_start as usize..name_end as usize]))
                }
                None if (kinds.contains(kind_id) || CALLEE_NAME_KINDS.contains(&kind_text))
                    && named_children == 0 =>
                {
                    Some(strings.intern(&src[start as usize..end as usize]))
                }
                None => None,
            };
            bundle.nodes.push(node);
            if let Some(parent) = parent {
                bundle
                    .edges
                    .push(Edge::new(NodeRef(parent), NodeRef(ix), CstEdgeKind::Child));
            }
        },
    );
    Some(bundle)
}
