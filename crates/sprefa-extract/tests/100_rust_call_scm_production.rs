use sprefa_extract::{FamilyMask, RustSource, Source};

const FIXTURE: &str = r#"
macro_rules! mint {
    ($name:ident) => { fn $name() { helper(); } };
}

fn free() {
    fn nested() {}
    let closure = || {};
}

struct Owner;
impl Owner {
    fn inherent(&self) {}
}

trait Trait {
    fn with_body(&self) {}
    fn signature_only(&self);
}

enum Choice { Variant }
fn helper() {}
const VALUE: () = helper();

#[cfg(test)]
mod tests {
    fn inherited_cfg() {}
}

mint!(generated);
"#;

#[test]
fn production_call_rows_cover_scm_and_focused_supplements() {
    let output = RustSource.extract(
        "rust_call_scm_production.rs",
        FIXTURE.as_bytes(),
        FamilyMask::ALL,
    );
    let call = output.call.as_ref().expect("call family");
    let mut rows: Vec<String> = call
        .nodes
        .iter()
        .map(|node| {
            let name = node.name.map(|id| output.strings.lookup(id)).unwrap_or("-");
            format!(
                "node {}..{} {:?} {name}",
                node.span.start,
                node.span.end(),
                node.kind
            )
        })
        .collect();
    rows.extend(call.aux.cfg_scopes.iter().map(|scope| {
        format!(
            "cfg {}..{} {}",
            scope.span.start,
            scope.span.end(),
            output.strings.lookup(scope.cfg)
        )
    }));
    rows.extend(call.aux.method_owners.iter().map(|owner| {
        let self_type = owner
            .self_type
            .map(|id| output.strings.lookup(id))
            .unwrap_or("-");
        let trait_name = owner
            .trait_name
            .map(|id| output.strings.lookup(id))
            .unwrap_or("-");
        format!(
            "owner {}..{} {self_type} {trait_name}",
            owner.span.start,
            owner.span.end(),
        )
    }));
    rows.sort();
    let actual = rows.join("\n");
    let expected = "cfg 364..382 test
node 124..129 Lambda -
node 168..186 Method inherent
node 211..230 Method with_body
node 238..259 Method signature_only
node 278..285 Free Variant
node 291..302 Free helper
node 309..329 Ext(LangKind { lang: \"rust\", tag: \"const_init\" }) VALUE
node 364..382 Free inherited_cfg
node 386..403 Free generated
node 78..132 Free free
node 94..105 Free nested
owner 168..186 Owner -
owner 211..230 - Trait
owner 238..259 - Trait";
    assert_eq!(actual, expected);
}
