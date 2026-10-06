use serde_json::{json, Value};
use sprefa_extract::{build_cfg, cfg_facts, dispatch, flatten_cfg, CfgRole, FamilyMask, RoleRule, KOTLIN_ROLES};

pub fn evaluate(case: &Value) -> Value {
    crate::fixture_runner::commands(case, |step| {
        let source = std::fs::read(crate::fixture_runner::expand_text(step["input"].as_str().unwrap(), "")).unwrap();
        let path = step["path"].as_str().unwrap();
        let result = if step["api"] == "role_tables" {
            let output = dispatch(path, &source, FamilyMask { cst:true, ..FamilyMask::NONE }).unwrap();
            let cst = output.cst.as_ref().unwrap();
            let roles: Vec<_> = step["stub_roles"].as_array().unwrap().iter().map(|row| (row[0].as_str().unwrap(), RoleRule::Fixed(match row[1].as_str().unwrap() {
                "callable" => CfgRole::Callable, "branch" => CfgRole::Branch, "loop" => CfgRole::Loop, "jump" => CfgRole::Jump, other => panic!("unknown role: {other}"),
            }))).collect();
            json!({"stub":flatten_cfg(&build_cfg(&roles, cst, &output.strings, &source)),"real":flatten_cfg(&build_cfg(KOTLIN_ROLES, cst, &output.strings, &source))})
        } else { json!(cfg_facts(path, &source)) };
        if let Some(expected) = step.get("expect") { assert_eq!(&result, expected); }
        result
    })
}
