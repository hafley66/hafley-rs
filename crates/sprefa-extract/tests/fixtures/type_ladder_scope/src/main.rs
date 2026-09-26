pub fn reexport_chain() -> type_ladder_scope::bridge_mod::PubThing {
    type_ladder_scope::bridge_mod::PubThing
}

use type_ladder_scope::String;

pub fn external_shadow() -> String {
    String::new()
}

fn main() {}
