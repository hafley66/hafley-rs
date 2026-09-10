use game_content::{decode_file, emit_chart, TransitionSpec, Trigger};
use std::path::{Path, PathBuf};

fn falcon_source() -> Result<String, Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/fighters/falcon/imported");
    let files = [
        "Wait1.html", "JumpF.html", "AttackAirF.html",
        "JumpSquat.html", "Fall.html", "LandingAirF.html",
        "LandingHeavy.html",
    ];
    let actions = files.iter().map(|file| decode_file(&root.join(file)))
        .collect::<Result<Vec<_>, _>>()?;
    let transitions = [
        TransitionSpec { from: "JumpSquat", trigger: Trigger::Complete, to: "JumpF" },
        TransitionSpec { from: "JumpF", trigger: Trigger::Complete, to: "Fall" },
        TransitionSpec { from: "AttackAirF", trigger: Trigger::Complete, to: "Fall" },
        TransitionSpec { from: "LandingAirF", trigger: Trigger::Complete, to: "Wait1" },
        TransitionSpec { from: "LandingHeavy", trigger: Trigger::Complete, to: "Wait1" },
        TransitionSpec { from: "Wait1", trigger: Trigger::JumpPress, to: "JumpSquat" },
        TransitionSpec { from: "JumpF", trigger: Trigger::AttackEligible, to: "AttackAirF" },
        TransitionSpec { from: "Fall", trigger: Trigger::AttackEligible, to: "AttackAirF" },
        TransitionSpec { from: "AttackAirF", trigger: Trigger::AttackEligible, to: "AttackAirF" },
        TransitionSpec { from: "AttackAirF", trigger: Trigger::LandDuringAttack, to: "LandingAirF" },
        TransitionSpec { from: "JumpF", trigger: Trigger::Land, to: "LandingHeavy" },
        TransitionSpec { from: "Fall", trigger: Trigger::Land, to: "LandingHeavy" },
    ];
    Ok(emit_chart(&actions, &transitions)?)
}

fn falcon(output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(output, falcon_source()?)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let fighter = args.next().ok_or("usage: smash-import falcon [output]")?;
    if fighter != "falcon" || args.len() > 1 {
        return Err("usage: smash-import falcon [output]".into());
    }
    let output = args.next().map(PathBuf::from).unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/fighters/falcon/generated/0_chart.rs")
    });
    falcon(&output)
}

#[cfg(test)]
mod tests {
    #[test]
    fn generated_falcon_chart_is_current() {
        assert_eq!(
            super::falcon_source().unwrap(),
            include_str!("fighters/falcon/generated/0_chart.rs"),
        );
    }
}
