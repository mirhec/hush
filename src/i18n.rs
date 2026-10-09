//! Localized application text. GitHub content and interpolated diagnostics stay unchanged.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::LazyLock};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    System,
    De,
    En,
    Es,
    Fr,
    Pt,
    Zh,
    Ja,
}

impl Language {
    pub const ALL: [Self; 8] = [
        Self::System,
        Self::De,
        Self::En,
        Self::Es,
        Self::Fr,
        Self::Pt,
        Self::Zh,
        Self::Ja,
    ];

    pub fn native_name(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::De => "Deutsch",
            Self::En => "English",
            Self::Es => "Español",
            Self::Fr => "Français",
            Self::Pt => "Português",
            Self::Zh => "简体中文",
            Self::Ja => "日本語",
        }
    }

    pub fn from_locale(locale: &str) -> Self {
        match locale
            .trim()
            .split(['-', '_', '.', '@'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "de" => Self::De,
            "en" => Self::En,
            "es" => Self::Es,
            "fr" => Self::Fr,
            "pt" => Self::Pt,
            "zh" => Self::Zh,
            "ja" => Self::Ja,
            _ => Self::En,
        }
    }

    pub fn resolved(self) -> Self {
        if self == Self::System {
            sys_locale::get_locale()
                .as_deref()
                .map(Self::from_locale)
                .unwrap_or(Self::En)
        } else {
            self
        }
    }

    fn catalog(self) -> &'static Catalog {
        let index = match self.resolved() {
            Self::De => 0,
            Self::En | Self::System => 1,
            Self::Es => 2,
            Self::Fr => 3,
            Self::Pt => 4,
            Self::Zh => 5,
            Self::Ja => 6,
        };
        &CATALOGS[index]
    }

    pub fn text(self, key: &str) -> &str {
        self.catalog()
            .get(key)
            .or_else(|| CATALOGS[1].get(key))
            .map(String::as_str)
            .unwrap_or(key)
    }

    /// Substitute once, so braces inside values cannot become new placeholders.
    pub fn format(self, key: &str, args: &[(&str, &str)]) -> String {
        render(self.text(key), |name| {
            args.iter()
                .find_map(|(key, value)| (*key == name).then_some(*value))
        })
    }

    /// Translate known application diagnostics, including ones persisted by older builds.
    /// Callers must not use this to translate GitHub titles, comments, or other user content.
    pub fn message(self, text: &str) -> String {
        self.resolved().message_inner(text, 0)
    }

    fn message_inner(self, text: &str, depth: usize) -> String {
        if self == Self::De || text.len() > 65_536 || depth >= 4 {
            return text.to_owned();
        }
        if CATALOGS[0].contains_key(text) {
            return self.text(text).to_owned();
        }
        for template in TEMPLATES.iter() {
            let mut captures = Vec::new();
            let mut budget = 256;
            if match_parts(&template.parts, text, &mut captures, &mut budget) {
                // Only these named slots contain application errors. Other captures may
                // contain paths, GitHub content, or identifiers and must remain verbatim.
                let captures: Vec<_> = captures
                    .into_iter()
                    .map(|(name, value)| {
                        let value = if matches!(name, "e" | "error") {
                            std::borrow::Cow::Owned(self.message_inner(value, depth + 1))
                        } else {
                            std::borrow::Cow::Borrowed(value)
                        };
                        (name, value)
                    })
                    .collect();
                return render(self.text(template.key), |name| {
                    captures
                        .iter()
                        .find_map(|(key, value)| (*key == name).then_some(value.as_ref()))
                });
            }
        }
        // Repository warning prefixes come from validated Repo values, optionally
        // followed by one of the supported GitHub subject types. Only ApiError
        // suffixes qualify; this is not a general "anything: translated text" rule.
        if let Some((prefix, error)) = text.split_once(": ")
            && repository_warning_prefix(prefix)
            && known_api_error(error)
        {
            return format!("{prefix}: {}", self.message_inner(error, depth + 1));
        }
        // Anyhow's display chains use this separator. Only known exact prefixes qualify.
        for (offset, _) in text.match_indices(": ") {
            let prefix = &text[..offset];
            if CATALOGS[0].contains_key(prefix) {
                return format!(
                    "{}: {}",
                    self.text(prefix),
                    self.message_inner(&text[offset + 2..], depth + 1)
                );
            }
        }
        text.to_owned()
    }
}

fn repository_warning_prefix(prefix: &str) -> bool {
    let repository = if let Some((repository, subject)) = prefix.split_once(" · ") {
        if !matches!(subject, "Issue" | "PullRequest" | "Commit" | "Discussion") {
            return false;
        }
        repository
    } else {
        prefix
    };
    crate::model::Repo::parse(repository)
        .is_ok_and(|repo| repo.to_string().eq_ignore_ascii_case(repository))
}

fn known_api_error(text: &str) -> bool {
    // Keep this list aligned with ApiError. The regression test covers every variant.
    const KEYS: [&str; 9] = [
        "GitHub-Token ungültig oder abgelaufen. Bitte neu verbinden.",
        "GitHub verweigert den Zugriff (HTTP {0}). Bitte GitHub-Anmeldung und Organisationsfreigaben prüfen.",
        "GitHub-Abfragelimit erreicht. Automatischer neuer Versuch nach der Wartezeit.",
        "Das Abfragebudget dieses Durchlaufs ist aufgebraucht. Verbleibende Threads werden später verarbeitet.",
        "Netzwerkfehler oder Zeitüberschreitung beim Kontakt mit GitHub.",
        "GitHub lieferte eine unerwartete Antwort (HTTP {0}).",
        "Antwort zu groß oder ungültig. Sync-Zeitpunkt bleibt unverändert.",
        "Seitengrenze erreicht: Abfrage wurde nicht als vollständig bestätigt.",
        "Unerlaubtes API-Ziel wurde blockiert.",
    ];
    KEYS.iter().any(|key| {
        if !key.contains('{') {
            return text == *key;
        }
        let parts = parts(key);
        let mut captures = Vec::new();
        match_parts(&parts, text, &mut captures, &mut 32)
            && captures.iter().all(|(_, value)| {
                !value.is_empty()
                    && value.bytes().all(|byte| byte.is_ascii_digit())
                    && value.parse::<u16>().is_ok()
            })
    })
}

type Catalog = BTreeMap<String, String>;

const SOURCES: [&str; 7] = [
    include_str!("locales/de.json"),
    include_str!("locales/en.json"),
    include_str!("locales/es.json"),
    include_str!("locales/fr.json"),
    include_str!("locales/pt.json"),
    include_str!("locales/zh.json"),
    include_str!("locales/ja.json"),
];

static CATALOGS: LazyLock<[Catalog; 7]> = LazyLock::new(|| {
    SOURCES.map(|source| serde_json::from_str(source).expect("valid embedded language catalog"))
});

#[derive(Debug)]
enum Part<'a> {
    Literal(&'a str),
    Slot { name: String, raw: &'a str },
}

fn parts(text: &str) -> Vec<Part<'_>> {
    let mut result = Vec::new();
    let mut offset = 0;
    let mut positional = 0;
    while let Some(start) = text[offset..].find('{').map(|index| offset + index) {
        let Some(end) = text[start + 1..].find('}').map(|index| start + 1 + index) else {
            return vec![Part::Literal(text)];
        };
        let raw = &text[start..=end];
        let name = text[start + 1..end].split(':').next().unwrap_or_default();
        if !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            return vec![Part::Literal(text)];
        }
        if start > offset {
            result.push(Part::Literal(&text[offset..start]));
        }
        let name = if name.is_empty() {
            let name = positional.to_string();
            positional += 1;
            name
        } else {
            name.to_owned()
        };
        result.push(Part::Slot { name, raw });
        offset = end + 1;
    }
    if offset < text.len() {
        result.push(Part::Literal(&text[offset..]));
    }
    result
}

fn render<'a>(template: &str, mut value: impl FnMut(&str) -> Option<&'a str>) -> String {
    let mut output = String::with_capacity(template.len());
    for part in parts(template) {
        match part {
            Part::Literal(text) => output.push_str(text),
            Part::Slot { name, raw } => output.push_str(value(&name).unwrap_or(raw)),
        }
    }
    output
}

struct Template {
    key: &'static str,
    parts: Vec<Part<'static>>,
    specificity: usize,
}

static TEMPLATES: LazyLock<Vec<Template>> = LazyLock::new(|| {
    let mut templates: Vec<_> = CATALOGS[0]
        .keys()
        .filter(|key| key.contains('{'))
        .map(|key| {
            let parts = parts(key);
            let specificity = parts
                .iter()
                .map(|part| match part {
                    Part::Literal(text) => text.len(),
                    Part::Slot { .. } => 0,
                })
                .sum();
            Template {
                key,
                parts,
                specificity,
            }
        })
        // A template must contain actual application wording, not just placeholders.
        .filter(|template| template.specificity >= 4)
        .collect();
    templates.sort_by_key(|template| std::cmp::Reverse(template.specificity));
    templates
});

fn match_parts<'a, 'b>(
    parts: &'a [Part<'_>],
    text: &'b str,
    captures: &mut Vec<(&'a str, &'b str)>,
    budget: &mut usize,
) -> bool {
    if *budget == 0 {
        return false;
    }
    *budget -= 1;
    let Some((part, rest)) = parts.split_first() else {
        return text.is_empty();
    };
    match part {
        Part::Literal(literal) => text
            .strip_prefix(literal)
            .is_some_and(|tail| match_parts(rest, tail, captures, budget)),
        Part::Slot { name, .. } => {
            let try_capture =
                |end: usize, captures: &mut Vec<(&'a str, &'b str)>, budget: &mut usize| {
                    let value = &text[..end];
                    if matches!(
                        name.as_str(),
                        "count" | "remaining" | "pending" | "unread" | "m"
                    ) && (value.is_empty() || !value.chars().all(|ch| ch.is_ascii_digit()))
                    {
                        return false;
                    }
                    if let Some((_, previous)) = captures.iter().find(|(key, _)| *key == name)
                        && *previous != value
                    {
                        return false;
                    }
                    captures.push((name.as_str(), value));
                    if match_parts(rest, &text[end..], captures, budget) {
                        true
                    } else {
                        captures.pop();
                        false
                    }
                };
            match rest.first() {
                None => try_capture(text.len(), captures, budget),
                Some(Part::Literal(delimiter)) => text
                    .match_indices(delimiter)
                    .any(|(end, _)| try_capture(end, captures, budget)),
                // Adjacent placeholders are ambiguous and are not used by our catalogs.
                Some(Part::Slot { .. }) => false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_locales_and_falls_back_to_english() {
        for (locale, expected) in [
            ("de-DE", Language::De),
            ("de_DE.UTF-8", Language::De),
            ("EN_us", Language::En),
            ("es-MX", Language::Es),
            ("fr_CA@euro", Language::Fr),
            ("pt_BR", Language::Pt),
            ("zh-Hans-CN", Language::Zh),
            ("zh-Hant-TW", Language::Zh),
            ("ja_JP.UTF-8", Language::Ja),
            (" C ", Language::En),
            ("ru_RU", Language::En),
            ("", Language::En),
        ] {
            assert_eq!(Language::from_locale(locale), expected, "{locale}");
        }
        assert_ne!(Language::System.resolved(), Language::System);
        assert_eq!(Language::default(), Language::System);
        assert_eq!(serde_json::to_string(&Language::Zh).unwrap(), "\"zh\"");
    }

    #[test]
    fn all_catalogs_cover_the_same_keys_and_preserve_placeholders() {
        fn placeholders(value: &str) -> Vec<&str> {
            let mut values: Vec<_> = parts(value)
                .into_iter()
                .filter_map(|part| match part {
                    Part::Slot { raw, .. } => Some(raw),
                    Part::Literal(_) => None,
                })
                .collect();
            values.sort_unstable();
            values
        }
        for (index, catalog) in CATALOGS.iter().enumerate() {
            assert_eq!(
                catalog.keys().collect::<Vec<_>>(),
                CATALOGS[0].keys().collect::<Vec<_>>(),
                "catalog {index}"
            );
            for (key, value) in catalog {
                assert!(!value.trim().is_empty(), "catalog {index}, {key}");
                assert_eq!(
                    placeholders(key),
                    placeholders(value),
                    "catalog {index}, {key}"
                );
            }
        }
    }

    #[test]
    fn translates_exact_text_and_formats_values_only_once() {
        assert_eq!(Language::En.text("Abbrechen"), "Cancel");
        assert_eq!(
            Language::En.text("Unrecognized {text}"),
            "Unrecognized {text}"
        );
        assert_eq!(
            Language::En.format(
                "System ({language})",
                &[("language", "{count} https://github.com/a/{b}")]
            ),
            "System ({count} https://github.com/a/{b})"
        );
        assert_eq!(
            Language::En.format("{count} ungelesen", &[("count", "20")]),
            "20 unread"
        );
        assert_eq!(
            Language::En.format("{count} ungelesen", &[]),
            "{count} unread"
        );
        assert_eq!(
            Language::En.format(
                "{} · {} API-Aufrufe · {} Teams",
                &[("0", "2026"), ("1", "10"), ("2", "2")]
            ),
            "2026 · 10 API requests · 2 teams"
        );
    }

    #[test]
    fn translates_persisted_diagnostics_without_rewriting_interpolated_values() {
        assert_eq!(
            Language::En.message("Einstellungen gespeichert."),
            "Settings saved."
        );
        assert_eq!(
            Language::En.message("GitHub lieferte eine unerwartete Antwort (HTTP 503)."),
            "GitHub returned an unexpected response (HTTP 503)."
        );
        assert_eq!(
            Language::En.message("Review für dein Team @a-team angefragt."),
            "Review requested from your team @a-team."
        );
        assert_eq!(
            Language::En.message(
                "Tray konnte nicht gestartet werden: https://github.com/{error:#}: Einstellungen"
            ),
            "Could not start the tray: https://github.com/{error:#}: Einstellungen"
        );
        assert_eq!(
            Language::En
                .message("Fenster wurde beendet (exit status: 1). Details: /tmp/{path}/Abbrechen"),
            "Window exited (exit status: 1). Details: /tmp/{path}/Abbrechen"
        );
        assert_eq!(Language::En.message("Hintergrunddienst wurde beendet (exit status: 1). disk: full Protokoll: /tmp/Protokoll: {0}"), "Background service exited (exit status: 1). disk: full Log: /tmp/Protokoll: {0}");
        assert_eq!(Language::En.message("Schlüsselbund nicht verfügbar.: Token konnte nicht aus dem Schlüsselbund gelöscht werden. Bitte dort manuell entfernen und gegebenenfalls auf GitHub widerrufen."), "Keyring unavailable.: Could not delete the token from the keyring. Please remove it manually and revoke it on GitHub if needed.");
    }

    #[test]
    fn translates_api_diagnostics_inside_known_error_fields_and_repository_warnings() {
        use crate::api::ApiError;
        for error in [
            ApiError::Unauthorized,
            ApiError::Access(403),
            ApiError::RateLimit(123),
            ApiError::Budget,
            ApiError::Network,
            ApiError::Http(503),
            ApiError::Invalid,
            ApiError::Pagination,
            ApiError::Origin,
        ] {
            let error = error.to_string();
            let translated = Language::En.message(&error);
            assert_ne!(error, translated);
            for prefix in [
                "Example/repo",
                "Example/repo · Issue",
                "Example/repo · PullRequest",
                "Example/repo · Commit",
                "Example/repo · Discussion",
            ] {
                assert_eq!(
                    Language::En.message(&format!("{prefix}: {error}")),
                    format!("{prefix}: {translated}")
                );
            }
            assert_eq!(
                Language::En.message(&format!("Automatische Teams: {error}")),
                format!("Automatic teams: {translated}")
            );
            assert_eq!(
                Language::En.message(&format!("Eigene Pull Requests: {error}")),
                format!("Your pull requests: {translated}")
            );
            assert_eq!(
                Language::En.message(&format!("Heartbeat: {error}")),
                format!("Heartbeat: {translated}")
            );
        }
        let error = ApiError::Http(500).to_string();
        for prefix in [
            "/tmp/repo",
            "https://github.com/example/repo",
            "example/repo/path",
            " example/repo",
            "example/repo ",
            "example/repo · Unknown",
            "example/repo · Issue · PullRequest",
        ] {
            let original = format!("{prefix}: {error}");
            assert_eq!(Language::En.message(&original), original);
        }
        for suffix in [
            "Abbrechen",
            "Einstellungen gespeichert.",
            "GitHub lieferte eine unerwartete Antwort (HTTP {0}).",
            "GitHub lieferte eine unerwartete Antwort (HTTP /tmp/{path}).",
            "unknown error: Einstellungen gespeichert.",
        ] {
            let original = format!("example/repo: {suffix}");
            assert_eq!(Language::En.message(&original), original);
        }
        assert_eq!(
            Language::En.message("Automatische Teams: /tmp/{e}/Einstellungen gespeichert."),
            "Automatic teams: /tmp/{e}/Einstellungen gespeichert."
        );
    }

    #[test]
    fn leaves_unknown_messages_and_non_numeric_counts_unchanged() {
        for text in [
            "A user wrote Einstellungen gespeichert. in a comment",
            "prefix GitHub lieferte eine unerwartete Antwort (HTTP 503). suffix",
            "https://github.com/Abbrechen/{count}",
            "No translation: Einstellungen gespeichert.",
            "someone ungelesen",
            "{count} unknown",
        ] {
            assert_eq!(Language::En.message(text), text);
        }
        let original = "Tray konnte nicht gestartet werden: {error:#}";
        assert_eq!(Language::De.message(original), original);
    }
}
