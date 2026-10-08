//! Pure, testable event predicates. A sticky GitHub notification reason is NEVER an event.
use crate::model::{Event, Kind, Repo, parse_time};
use chrono::{DateTime, Utc};
use pulldown_cmark::{Event as MdEvent, Parser, Tag, TagEnd};
use serde_json::Value;
use std::collections::HashSet;

pub fn safe_api_url(raw: &str) -> anyhow::Result<url::Url> {
    let u = url::Url::parse(raw)?;
    anyhow::ensure!(u.scheme() == "https" && u.host_str() == Some("api.github.com")
        && u.username().is_empty() && u.password().is_none() && u.port().is_none(),
        "API-Adresse außerhalb von api.github.com wurde blockiert.");
    anyhow::ensure!(u.fragment().is_none(), "API-Adresse mit Fragment wurde blockiert.");
    Ok(u)
}
pub fn safe_web_url(raw: &str) -> anyhow::Result<url::Url> {
    let u = url::Url::parse(raw)?;
    anyhow::ensure!(u.scheme() == "https" && u.host_str() == Some("github.com")
        && u.username().is_empty() && u.password().is_none() && u.port().is_none(),
        "Nur HTTPS-Links auf github.com dürfen geöffnet werden.");
    Ok(u)
}

/// Ignore inline/fenced code, HTML and quoted text; match a complete, case-insensitive handle.
/// This approximates rendered Markdown mentions; GitHub's own mention parser is not public.
pub fn mentions(body: &str, login: &str) -> bool {
    if login.is_empty() { return false; }
    let target = format!("@{}", login.to_ascii_lowercase());
    let mut ignored = 0usize;
    let mut text = String::new();
    for e in Parser::new(body) {
        match e {
            MdEvent::Start(Tag::CodeBlock(_) | Tag::BlockQuote(_)) => ignored += 1,
            MdEvent::End(TagEnd::CodeBlock | TagEnd::BlockQuote(_)) => ignored = ignored.saturating_sub(1),
            MdEvent::Text(t) if ignored == 0 => { text.push_str(&t); },
            MdEvent::SoftBreak | MdEvent::HardBreak => text.push(' '),
            _ => { text.push(' '); }
        }
    }
    let text = text.to_ascii_lowercase();
    text.match_indices(&target).any(|(start, _)| {
        let before = text[..start].chars().next_back();
        let after = text[start + target.len()..].chars().next();
        let token = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '/' | '@');
        before.is_none_or(|c| !token(c)) && after.is_none_or(|c| !token(c))
    })
}

pub fn plain_excerpt(markdown: &str) -> String {
    let mut out = String::new();
    for event in Parser::new(markdown) {
        match event {
            MdEvent::Text(text) | MdEvent::Code(text) => out.push_str(&text),
            MdEvent::SoftBreak | MdEvent::HardBreak | MdEvent::End(_) => out.push(' '),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(600).collect()
}

pub fn str_at<'a>(value: &'a Value, path: &[&str]) -> &'a str {
    let mut current = value;
    for key in path { current = &current[*key]; }
    current.as_str().unwrap_or("")
}
pub fn object_id(v: &Value) -> Option<String> {
    v.get("id").and_then(|id| if let Some(s) = id.as_str() { Some(s.to_owned()) } else { id.as_u64().map(|n| n.to_string()) })
}
fn actor(v: &Value) -> String {
    let a = str_at(v, &["user", "login"]);
    if a.is_empty() { str_at(v, &["actor", "login"]).to_owned() } else { a.to_owned() }
}
fn event(kind: Kind, id: String, repo: &Repo, title: &str, actor: &str, detail: String, raw_url: &str, at: DateTime<Utc>) -> Option<Event> {
    let url = safe_web_url(raw_url).ok()?.to_string();
    Some(Event { id, kind, title: title.chars().take(300).collect(), repository: repo.to_string(),
        actor: actor.to_owned(), detail, url, occurred_at: at, unread: true, archived: false })
}

pub fn new_issue(v: &Value, repo: &Repo, since: DateTime<Utc>) -> Option<Event> {
    if v.get("pull_request").is_some() { return None; }
    let at = parse_time(str_at(v, &["created_at"]))?;
    if at < since { return None; }
    event(Kind::Issue, format!("issue:{}", object_id(v)?), repo, str_at(v, &["title"]),
        &actor(v), plain_excerpt(str_at(v, &["body"])), str_at(v, &["html_url"]), at)
}

pub fn comment_mention(v: &Value, namespace: &str, repo: &Repo, title: &str, login: &str, since: DateTime<Utc>) -> Option<Event> {
    let body = str_at(v, &["body"]);
    let who = actor(v);
    if who.eq_ignore_ascii_case(login) || !mentions(body, login) { return None; }
    let at = parse_time(str_at(v, &["updated_at"]))
        .or_else(|| parse_time(str_at(v, &["created_at"])))
        .or_else(|| parse_time(str_at(v, &["submitted_at"])))?;
    if at < since { return None; }
    event(Kind::Mention, format!("mention:{namespace}:{}", object_id(v)?), repo, title,
        &who, plain_excerpt(body), str_at(v, &["html_url"]), at)
}

pub fn review(v: &Value, repo: &Repo, title: &str, pr_author: &str, login: &str, since: DateTime<Utc>) -> Option<Event> {
    if !pr_author.eq_ignore_ascii_case(login) || actor(v).eq_ignore_ascii_case(login) { return None; }
    let at = parse_time(str_at(v, &["submitted_at"]))?;
    if at < since { return None; }
    let state = str_at(v, &["state"]);
    if !["APPROVED", "CHANGES_REQUESTED", "COMMENTED"].contains(&state) { return None; }
    let label = match state { "APPROVED" => "Freigegeben", "CHANGES_REQUESTED" => "Änderungen angefragt", _ => "Review kommentiert" };
    let snippet = plain_excerpt(str_at(v, &["body"]));
    let detail = if snippet.is_empty() { label.to_owned() } else { format!("{label} · {snippet}") };
    event(Kind::Review, format!("review:{}", object_id(v)?), repo, title, &actor(v), detail, str_at(v, &["html_url"]), at)
}

pub fn request(v: &Value, repo: &Repo, title: &str, web_url: &str, login: &str, teams: &HashSet<String>, since: DateTime<Utc>) -> Option<Event> {
    let at = parse_time(str_at(v, &["created_at"]))?;
    if at < since { return None; }
    let action = str_at(v, &["event"]);
    let detail = match action {
        "assigned" if str_at(v, &["assignee", "login"]).eq_ignore_ascii_case(login) => "Dir wurde dieser Pull Request zugewiesen.".to_owned(),
        "review_requested" => {
            let reviewer = str_at(v, &["requested_reviewer", "login"]);
            let team_slug = str_at(v, &["requested_team", "slug"]);
            let slug = if team_slug.is_empty() { str_at(v, &["requested_reviewer", "slug"]) } else { team_slug };
            // The team belongs to the repository's organisation. Do not trust an unrelated team name.
            let team = format!("{}/{}", repo.owner, slug).to_ascii_lowercase();
            if reviewer.eq_ignore_ascii_case(login) { "Dein Review wurde angefragt.".to_owned() }
            else if !slug.is_empty() && teams.contains(&team) { format!("Review für dein Team @{team} angefragt.") }
            else { return None; }
        }
        _ => return None,
    };
    event(Kind::Request, format!("request:{}:{}", repo, object_id(v)?), repo, title, &actor(v), detail, web_url, at)
}

/// Convert a notification subject URL only after validating its host AND repository path.
pub fn subject_locator(raw: &str, repo: &Repo) -> anyhow::Result<(String, String)> {
    let url = safe_api_url(raw)?;
    let prefix = format!("{}/", repo.api_path());
    anyhow::ensure!(url.path().to_ascii_lowercase().starts_with(&prefix), "Repository der Benachrichtigung stimmt nicht mit dem API-Ziel überein.");
    let tail = &url.path()[prefix.len()..];
    let parts: Vec<_> = tail.split('/').collect();
    anyhow::ensure!(parts.len() == 2, "Nicht unterstützter GitHub-Thread.");
    let kind = parts[0];
    let id = parts[1];
    let valid = match kind {
        "issues" | "pulls" | "discussions" => !id.is_empty() && id.bytes().all(|c| c.is_ascii_digit()),
        "commits" => (7..=64).contains(&id.len()) && id.bytes().all(|c| c.is_ascii_hexdigit()),
        _ => false,
    };
    anyhow::ensure!(valid, "Nicht unterstützter GitHub-Thread.");
    Ok((kind.to_owned(), id.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn exact_handle() {
        assert!(mentions("Hey @Alice, bitte ansehen!", "alice"));
        for body in ["@alice-bot", "mail@alice.com", "@alice/team", "`@alice`", "```\n@alice\n```", "> @alice", "@alices"] {
            assert!(!mentions(body, "alice"), "unexpected match: {body}");
        }
    }
    #[test] fn origins_are_exact() {
        assert!(safe_api_url("https://api.github.com/repos/acme/api/issues/1").is_ok());
        for u in ["http://api.github.com/x", "https://api.github.com.evil.test/x", "https://api.github.com@evil.test/x", "https://api.github.com:8443/x", "file:///etc/passwd"] {
            assert!(safe_api_url(u).is_err(), "accepted {u}");
        }
        assert!(safe_web_url("javascript:alert(1)").is_err());
    }
    #[test] fn repo_paths_are_not_commands() {
        for repo in ["a/../b", "a/..", "a/$(whoami)", "a/b?token=x", "a/b/c", "https://github.com/a/b"] {
            assert!(Repo::parse(repo).is_err());
        }
    }
    #[test] fn only_requested_team() {
        let repo = Repo::parse("acme/api").unwrap();
        let at = Utc::now();
        let v = serde_json::json!({"id": 1,"created_at":at.to_rfc3339(),"event":"review_requested","requested_team":{"slug":"other"}});
        let teams = HashSet::from(["acme/platform".to_owned()]);
        assert!(request(&v,&repo,"Title","https://github.com/acme/api/pull/1","alice",&teams,at-chrono::Duration::seconds(1)).is_none());
    }
}
