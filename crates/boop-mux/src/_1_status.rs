//! Live tmux status geometry, independent of status text and colour.
use crate::{exact_pane_target, tmux_command, Tmux};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusGeometry {
    pub position: String,
    pub rows: u16,
}

pub fn parse_status_geometry(text: &str) -> Option<StatusGeometry> {
    let (status, position) = text.trim().split_once('\t')?;
    if !matches!(position, "top" | "bottom") { return None; }
    let rows = match status {
        "off" => 0,
        "on" => 1,
        value => value.parse::<u16>().ok()?.min(5),
    };
    Some(StatusGeometry { position: position.to_owned(), rows })
}

impl Tmux {
    pub fn status_geometry(&self, socket: Option<&str>, target: &str) -> Option<StatusGeometry> {
        let target = exact_pane_target(target);
        let output = tmux_command(socket)
            .args(["display-message", "-p", "-t", &target, "#{session_name}\t#{status}\t#{status-position}"])
            .output().ok()?;
        if !output.status.success() { return None; }
        let text = String::from_utf8_lossy(&output.stdout);
        let (session, status) = text.split_once('\t')?;
        if session.is_empty() { return None; }
        parse_status_geometry(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_can_move_change_height_or_disappear() {
        for (raw, expected) in [("on\ttop", Some(("top", 1))), ("2\tbottom", Some(("bottom", 2))),
            ("off\ttop", Some(("top", 0))), ("0\tbottom", Some(("bottom", 0))), ("?\ttop", None)] {
            assert_eq!(parse_status_geometry(raw).map(|g| (g.position, g.rows)), expected.map(|(p, r)| (p.to_owned(), r)));
        }
    }
    #[test]
    fn reads_changes_from_an_isolated_tmux_server() {
        struct Server(String);
        impl Drop for Server {
            fn drop(&mut self) { crate::kill_test_server(&self.0); }
        }
        let server = Server(format!("boop-status-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let started = tmux_command(Some(&server.0))
            .args(["-f", "/dev/null", "new-session", "-d", "-s", "geometry", "-x", "80", "-y", "24", "/bin/sleep 30"])
            .output().unwrap();
        assert!(started.status.success(), "{}", String::from_utf8_lossy(&started.stderr));
        for (status, position, rows) in [("on", "bottom", 1), ("on", "top", 1), ("2", "top", 2), ("off", "top", 0)] {
            for (option, value) in [("status", status), ("status-position", position)] {
                assert!(tmux_command(Some(&server.0)).args(["set-option", "-t", "geometry", option, value]).status().unwrap().success());
            }
            assert_eq!(Tmux.status_geometry(Some(&server.0), "geometry"), Some(StatusGeometry { position: position.into(), rows }));
        }
    }

}
