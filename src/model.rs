use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const APP_ID: &str = "io.hush.github";
pub const MAX_EVENTS: usize = 500;
pub const RETENTION_DAYS: i64 = 30;
pub const MAX_PAGES: usize = 10;
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Request,
    Issue,
    Review,
    Mention,
}
impl Kind {
    pub const ALL: [Self; 4] = [Self::Request, Self::Issue, Self::Review, Self::Mention];
    pub fn key(self) -> &'static str {
        match self { Self::Request => "request", Self::Issue => "issue", Self::Review => "review", Self::Mention => "mention" }
    }
    pub fn label(self) -> &'static str {
        match self { Self::Request => "Review & Zuweisung", Self::Issue => "Neue Issues", Self::Review => "Deine PR-Reviews", Self::Mention => "Erwähnungen" }
    }
    pub fn short(self) -> &'static str {
        match self { Self::Request => "PR-Anfrage", Self::Issue => "Neues Issue", Self::Review => "Review erhalten", Self::Mention => "Du wurdest erwähnt" }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Rules {
    pub requests: bool,
    pub issues: bool,
    pub reviews: bool,
    pub mentions: bool,
}
impl Default for Rules {
    fn default() -> Self { Self { requests: true, issues: true, reviews: true, mentions: true } }
}
impl Rules {
    pub fn allows(&self, kind: Kind) -> bool {
        match kind { Kind::Request => self.requests, Kind::Issue => self.issues, Kind::Review => self.reviews, Kind::Mention => self.mentions }
    }
    pub fn get_mut(&mut self, kind: Kind) -> &mut bool {
        match kind { Kind::Request => &mut self.requests, Kind::Issue => &mut self.issues, Kind::Review => &mut self.reviews, Kind::Mention => &mut self.mentions }
    }
}

/// Configuration contains NO credential. Tokens live exclusively in the OS keyring.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub language: crate::i18n::Language,
    pub light_theme: bool,
    pub login: String,
    pub rules: Rules,
    pub repositories: Vec<Repo>,
    pub manual_teams: Vec<Repo>,
    pub interval_secs: u64,
    pub desktop_notifications: bool,
    pub show_preview: bool,
    pub paused_until: i64,
    pub has_detail_token: bool,
    pub oauth: bool,
    pub read_org: bool,
    pub revision: u64,
}
impl Default for Config {
    fn default() -> Self {
        Self { language: crate::i18n::Language::default(), light_theme: false, login: String::new(), rules: Rules::default(), repositories: vec![], manual_teams: vec![],
            interval_secs: 120, desktop_notifications: true, show_preview: false,
            paused_until: 0, has_detail_token: false, oauth: false, read_org: false, revision: 0 }
    }
}
impl Config {
    pub fn paused(&self) -> bool { self.paused_until > Utc::now().timestamp() }
    pub fn validate(&mut self) -> anyhow::Result<()> {
        anyhow::ensure!([60, 120, 300].contains(&self.interval_secs), "Intervall muss 60, 120 oder 300 Sekunden sein.");
        anyhow::ensure!(self.repositories.len() <= 20, "Maximal 20 Issue-Repositories, damit das API-Budget ausreicht.");
        self.repositories.sort_by_key(ToString::to_string);
        self.repositories.dedup();
        self.manual_teams.sort_by_key(ToString::to_string);
        self.manual_teams.dedup();
        Ok(())
    }
}

/// Validated owner/repository or organisation/team. Never interpolate untrusted paths.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Repo {
    pub owner: String,
    pub name: String,
}
impl Repo {
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        let value = value.trim();
        let parts: Vec<_> = value.split('/').collect();
        anyhow::ensure!(parts.len() == 2, "Erwartet: organisation/repository");
        let valid = |s: &str| !s.is_empty() && s.len() <= 100 && s != "." && s != ".."
            && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
        anyhow::ensure!(valid(parts[0]) && valid(parts[1]), "Ungültiger Repository- oder Teamname.");
        Ok(Self { owner: parts[0].to_ascii_lowercase(), name: parts[1].to_ascii_lowercase() })
    }
    pub fn list(text: &str) -> anyhow::Result<Vec<Self>> {
        let mut result = Vec::new();
        for item in text.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).filter(|s| !s.is_empty()) {
            let repo = Self::parse(item)?;
            if !result.contains(&repo) { result.push(repo); }
        }
        Ok(result)
    }
    pub fn api_path(&self) -> String { format!("/repos/{}/{}", self.owner, self.name) }
}
impl fmt::Display for Repo { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}/{}", self.owner, self.name) } }
impl TryFrom<String> for Repo { type Error = anyhow::Error; fn try_from(s: String) -> anyhow::Result<Self> { Self::parse(&s) } }
impl From<Repo> for String { fn from(r: Repo) -> Self { r.to_string() } }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// GitHub object ID + semantic kind. Not a notification-thread timestamp.
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub repository: String,
    pub actor: String,
    pub detail: String,
    pub url: String,
    pub occurred_at: DateTime<Utc>,
    pub unread: bool,
}
impl Event {
    pub fn excerpt(&self, max: usize) -> String { self.detail.chars().take(max).collect() }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryCheck {
    pub repository: String,
    pub checked_at: i64,
    pub error: Option<String>,
    pub imported: usize,
    pub initial_import: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeStatus {
    pub heartbeat: i64,
    pub last_sync: Option<i64>,
    pub next_sync: Option<i64>,
    pub phase: String,
    pub warnings: Vec<String>,
    pub requests_last_cycle: u32,
    pub remaining: Option<u32>,
    pub rate_reset: Option<i64>,
    pub notification_error: Option<String>,
    #[serde(default)]
    pub service_error: Option<String>,
    pub team_count: usize,
    #[serde(default)]
    pub repositories: Vec<RepositoryCheck>,
}
impl RuntimeStatus {
    pub fn alive(&self) -> bool { self.heartbeat > Utc::now().timestamp() - 45 }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadTask {
    pub id: String,
    pub repository: Repo,
    pub subject_type: String,
    pub title: String,
    pub subject_url: String,
    pub latest_comment_url: Option<String>,
    pub reason: String,
    pub since: DateTime<Utc>,
    pub quiet: bool,
}

pub fn parse_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value).ok().map(|t| t.with_timezone(&Utc))
}
pub fn utc_now() -> DateTime<Utc> { Utc::now() }
