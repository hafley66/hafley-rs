pub fn prelude_result() -> Result<(), ()> {
    Ok(())
}

pub fn local_result() -> crate::_0_alias::Result<()> {
    Ok(())
}

pub fn external_output() -> std::process::Output {
    std::process::Command::new("true").output().unwrap()
}

pub fn bridged() -> crate::_3_bridge::LocalThing {
    crate::_3_bridge::LocalThing
}
