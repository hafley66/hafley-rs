//! Pins the const/static initializer defs folded into the CallCollector walk:
//! which initializers mint CONST_INIT, in source order.

use sprefa_extract::{FamilyMask, RustSource, Source};

const FIXTURE: &str = r#"
fn helper() {}
fn bump() -> u8 { 0 }

const VALUE: () = helper();
static STATE: u8 = bump();
const PLAIN: u32 = 3;
const COVERED: u32 = { let g = || helper(); 0 };
const MULTI: u32 = helper() as u32 + bump() as u32;

mod inner_mod {
    const IN_MOD: () = crate::helper();
    fn in_mod() {}
}

fn holder() {
    const INNER: u32 = helper();
}
"#;

#[test]
fn const_init_defs_fold_into_call_collection() {
    let output = RustSource.extract(
        "rust_const_init_fold.rs",
        FIXTURE.as_bytes(),
        FamilyMask::ALL,
    );
    let call = output.call.as_ref().expect("call family");

    // VALUE: call in the initializer -> CONST_INIT. STATE: statics too.
    // PLAIN: no calls -> nothing. COVERED: the only call sits inside the
    // lambda's own def span, so it is already owned -> nothing. MULTI: two
    // uncovered calls, one node. IN_MOD: inline-mod scope is walked. INNER:
    // fn-body scope never minted one.
    let inits: Vec<&str> = call
        .nodes
        .iter()
        .filter(|node| format!("{:?}", node.kind).contains("const_init"))
        .map(|node| output.strings.lookup(node.name.expect("const_init named")))
        .collect();
    assert_eq!(inits, vec!["VALUE", "STATE", "MULTI", "IN_MOD"]);

    // Site output is unchanged: every call expression is still a site, the
    // covered and fn-body ones included.
    let helper_sites = call
        .aux
        .sites
        .iter()
        .filter(|site| output.strings.lookup(site.callee) == "helper")
        .count();
    let bump_sites = call
        .aux
        .sites
        .iter()
        .filter(|site| output.strings.lookup(site.callee) == "bump")
        .count();
    assert_eq!(
        helper_sites, 5,
        "VALUE + COVERED lambda + MULTI + IN_MOD + INNER"
    );
    assert_eq!(bump_sites, 2, "STATE + MULTI");
}
