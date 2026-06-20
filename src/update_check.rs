use futures::channel::mpsc::UnboundedSender;

const RELEASES_API_URL: &str =
    "https://api.github.com/repos/Kazhuu/ebc-battery-tester/releases/latest";

pub const RELEASES_PAGE_URL: &str = "https://github.com/Kazhuu/ebc-battery-tester/releases";

#[derive(Debug, Clone)]
pub enum UpdateCheckState {
    Checking,
    UpdateAvailable(String),
    UpToDate,
    Failed,
}

pub fn spawn_update_check(ctx: egui::Context, tx: UnboundedSender<UpdateCheckState>) {
    std::thread::spawn(move || {
        let state = match fetch_latest_tag() {
            Ok(tag) => {
                let latest = tag.trim_start_matches('v');
                if is_newer(latest, env!("CARGO_PKG_VERSION")) {
                    UpdateCheckState::UpdateAvailable(tag)
                } else {
                    UpdateCheckState::UpToDate
                }
            }
            Err(_) => UpdateCheckState::Failed,
        };
        tx.unbounded_send(state).ok();
        ctx.request_repaint();
    });
}

#[derive(serde::Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

fn fetch_latest_tag() -> Result<String, String> {
    let release: GitHubRelease = ureq::get(RELEASES_API_URL)
        .set(
            "User-Agent",
            &format!("ebc-battery-tester/{}", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    Ok(release.tag_name)
}

fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let mut it = s.splitn(3, '.');
    Some((
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    ))
}

fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}
