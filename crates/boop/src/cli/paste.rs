//! `boop beep paste`: a file into a pane the way a hand would paste it.
//!
//! A terminal carries text, so an image never crosses a pty. claude and codex
//! read the OS pasteboard themselves when their paste key is pressed, which is
//! the one path that hands a TUI a picture. boop owns that path: the file goes
//! on the pasteboard, then the key goes to the pane over tmux.
//!
//! Pasteboard write, candidates considered (2026-09-04):
//!
//! | candidate | verdict |
//! |---|---|
//! | `osascript` `set the clipboard to (read (POSIX file …) as «class PNGf»)` | chosen: no crate, 124 ms measured, PNG/JPEG/GIF/TIFF classes |
//! | `arboard` 3 (`set_image`) | needs the file decoded to RGBA first (`image` crate), re-encodes on write |
//! | `objc2-app-kit` `NSPasteboard` | exact, but pulls the AppKit bindings into boop-harness for one call |
//! | `pbcopy` | text only |

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

use boop::harness::HarnessId;
use boop::registry::Registry;
use boop::{bus, live};

use crate::cli::mail_dir;

/// The AppleScript class the pasteboard write reads the file as, by extension.
pub fn pasteboard_class(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "PNGf",
        "jpg" | "jpeg" => "JPEG",
        "gif" => "GIFf",
        "tif" | "tiff" => "TIFF",
        "bmp" => "BMPf",
        _ => return None,
    })
}

/// The shell-quoted spelling a pane receives when the file is not pasted as
/// an image: single quotes only around whitespace, `'` escaped.
pub fn path_arg(path: &Path) -> String {
    let text = path.to_string_lossy();
    if text.chars().any(char::is_whitespace) {
        format!("'{}'", text.replace('\'', "'\\''"))
    } else {
        text.into_owned()
    }
}

/// What one paste will do, decided before anything touches the OS.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PastePlan {
    /// Pasteboard gets the file as `class`, then `keys` goes to the pane.
    Image { class: &'static str, keys: &'static str },
    /// The quoted path is typed into the pane with no Enter.
    PathText(String),
}

pub fn plan(path: &Path, image_paste_keys: Option<&'static str>, as_path: bool) -> PastePlan {
    match (as_path, pasteboard_class(path), image_paste_keys) {
        (false, Some(class), Some(keys)) => PastePlan::Image { class, keys },
        _ => PastePlan::PathText(path_arg(path)),
    }
}

fn write_pasteboard(path: &Path, class: &str) -> Result<()> {
    let script = format!(
        "set the clipboard to (read (POSIX file \"{}\") as «class {class}»)",
        path.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"")
    );
    let output = Command::new("osascript")
        .args(["-e", &script])
        .output()
        .context("run osascript")?;
    if !output.status.success() {
        bail!(
            "pasteboard write failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn send_keys(pane: &str, keys: &str, literal: bool) -> Result<()> {
    let mut command = Command::new("tmux");
    command.args(["send-keys", "-t", pane]);
    if literal {
        command.arg("-l");
    }
    command.arg(keys);
    let status = command.status().context("run tmux send-keys")?;
    if !status.success() {
        bail!("tmux send-keys into {pane} exited {status}");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_paste(
    registry: &Registry,
    path: &Path,
    route: Option<&str>,
    pane: Option<&str>,
    harness: Option<&str>,
    as_path: bool,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    if !path.exists() {
        bail!("{} does not exist", path.display());
    }
    let (pane, harness_id) = match (route, pane) {
        (Some(route), _) => {
            let dir = mail_dir(mail_dir_arg)?;
            let routes = bus::read_routes(&dir)?;
            let found = routes
                .get(route)
                .with_context(|| format!("no registered route `{route}`"))?;
            let target = found
                .tmux
                .as_deref()
                .filter(|t| !t.is_empty())
                .with_context(|| format!("route `{route}` names no pane"))?;
            (
                live::pane_of_target(target).unwrap_or_else(|| target.to_owned()),
                found.harness,
            )
        }
        (None, Some(pane)) => {
            let id = match harness {
                Some(name) => Some(
                    name.parse::<HarnessId>()
                        .map_err(|error| anyhow::anyhow!("{error}"))?,
                ),
                None => Some(HarnessId::Claude),
            };
            (live::pane_of_target(pane).unwrap_or_else(|| pane.to_owned()), id)
        }
        (None, None) => bail!("name a recipient: --route <route> or --pane <target>"),
    };
    let keys = harness_id.and_then(|id| registry.get(id).capabilities().image_paste_keys);
    match plan(path, keys, as_path) {
        PastePlan::Image { class, keys } => {
            write_pasteboard(path, class)?;
            send_keys(&pane, keys, false)?;
            println!(
                "pasted {} as {class} into {pane}: pasteboard + {keys}",
                path.display()
            );
        }
        PastePlan::PathText(text) => {
            send_keys(&pane, &format!("{text} "), true)?;
            println!("typed {text} into {pane} (no Enter)");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// RECEIPT. An image for a harness with a paste key goes through the
    /// pasteboard; anything else, or `--as-path`, is typed as a quoted path.
    #[test]
    fn an_image_pastes_and_everything_else_is_typed() {
        let png = PathBuf::from("/tmp/Screenshot 2026-09-04 at 8.24.07 PM.png");
        assert_eq!(
            plan(&png, Some("C-v"), false),
            PastePlan::Image { class: "PNGf", keys: "C-v" }
        );
        assert_eq!(
            plan(&png, None, false),
            PastePlan::PathText("'/tmp/Screenshot 2026-09-04 at 8.24.07 PM.png'".into())
        );
        assert_eq!(
            plan(&png, Some("C-v"), true),
            PastePlan::PathText("'/tmp/Screenshot 2026-09-04 at 8.24.07 PM.png'".into())
        );
        assert_eq!(
            plan(Path::new("/tmp/notes.md"), Some("C-v"), false),
            PastePlan::PathText("/tmp/notes.md".into())
        );
        assert_eq!(pasteboard_class(Path::new("a.JPG")), Some("JPEG"));
        assert_eq!(path_arg(Path::new("/it's here/x.png")), "'/it'\\''s here/x.png'");
    }
}
