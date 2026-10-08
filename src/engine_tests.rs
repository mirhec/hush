use super::*;
use chrono::Duration as ChronoDuration;
use serde_json::json;

fn database() -> (tempfile::TempDir, Store, Config) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&Paths::at(dir.path().join("data")).unwrap()).unwrap();
    let cfg = store
        .save_config(&Config {
            login: "alex".into(),
            repositories: vec![Repo::parse("example/private").unwrap()],
            ..Default::default()
        })
        .unwrap();
    (dir, store, cfg)
}
fn issue(id: u64, at: chrono::DateTime<Utc>) -> serde_json::Value {
    json!({"id":id,"title":"Test issue","created_at":at.to_rfc3339(),"body":"Test", "user":{"login":"author"},"html_url":format!("https://github.com/example/private/issues/{id}")})
}

#[test]
fn three_new_issues_reach_outbox_and_are_acknowledged_only_after_delivery() {
    let (_dir, mut store, cfg) = database();
    let mut status = RuntimeStatus::default();
    let baseline = Utc::now() - ChronoDuration::minutes(10);
    // Existing history is imported once without desktop banners.
    let old = issue(1, baseline - ChronoDuration::minutes(2));
    sync_issues(&mut store, &cfg, &mut status, baseline, |_, _| {
        Ok(vec![old.clone()])
    })
    .unwrap();
    assert_eq!(store.events().unwrap().len(), 1);
    assert!(store.outbox().unwrap().is_empty());
    let fresh: Vec<_> = (2..=4)
        .map(|id| issue(id, Utc::now() - ChronoDuration::minutes(1)))
        .collect();
    sync_issues(&mut store, &cfg, &mut status, Utc::now(), |_, since| {
        assert_eq!(since, baseline - ChronoDuration::minutes(2));
        Ok(fresh.clone())
    })
    .unwrap();
    assert_eq!(store.outbox().unwrap().len(), 3);
    assert_eq!(status.repositories[0].imported, 3);
    assert!(!status.repositories[0].initial_import);
    deliver_pending(&store, &cfg, &mut status, |events, _| {
        assert_eq!(events.len(), 3);
        anyhow::bail!("Notification service unavailable")
    })
    .unwrap();
    assert_eq!(store.outbox().unwrap().len(), 3);
    assert!(status.notification_error.is_some());
    deliver_pending(&store, &cfg, &mut status, |events, _| {
        assert_eq!(events.len(), 3);
        Ok(())
    })
    .unwrap();
    assert!(store.outbox().unwrap().is_empty());
    assert!(status.notification_error.is_none());
    sync_issues(&mut store, &cfg, &mut status, Utc::now(), |_, _| {
        Ok(fresh.clone())
    })
    .unwrap();
    assert!(
        store.outbox().unwrap().is_empty(),
        "Repeated poll must not duplicate banners"
    );
}

#[test]
fn inaccessible_private_repository_is_visible_and_does_not_advance_cursor() {
    let (_dir, mut store, mut cfg) = database();
    cfg.repositories
        .push(Repo::parse("example/public").unwrap());
    let cfg = store.save_config(&cfg).unwrap();
    let mut status = RuntimeStatus::default();
    sync_issues(&mut store, &cfg, &mut status, Utc::now(), |repo, _| {
        if repo.to_string().ends_with("private") {
            Err(ApiError::Access(404))
        } else {
            Ok(vec![])
        }
    })
    .unwrap();
    assert!(
        status.repositories[0]
            .error
            .as_ref()
            .unwrap()
            .contains("404")
    );
    assert!(status.repositories[1].error.is_none());
    assert!(status.warnings[0].contains("example/private"));
    assert!(store.cursor("issues:example/private").unwrap().1);
    assert!(!store.cursor("issues:example/public").unwrap().1);
}

#[test]
fn empty_outbox_does_not_clear_failed_test_notification() {
    let (_dir, store, cfg) = database();
    let mut status = RuntimeStatus {
        notification_error: Some("No desktop service".into()),
        ..Default::default()
    };
    deliver_pending(&store, &cfg, &mut status, |_, _| {
        panic!("No actual delivery expected")
    })
    .unwrap();
    assert_eq!(
        status.notification_error.as_deref(),
        Some("No desktop service")
    );
}

#[test]
fn adding_detail_token_can_reuse_existing_notification_token() {
    let cfg = Config {
        login: "alex".into(),
        ..Default::default()
    };
    let selected = connection_token(&cfg, Zeroizing::new(String::new()), || {
        Ok(Some(Zeroizing::new("stored-test-value".into())))
    })
    .unwrap();
    assert_eq!(selected.as_str(), "stored-test-value");
    let selected = connection_token(
        &cfg,
        Zeroizing::new("replacement-test-value".into()),
        || panic!("Do not read old token on replacement"),
    )
    .unwrap();
    assert_eq!(selected.as_str(), "replacement-test-value");
    assert!(
        connection_token(
            &Config::default(),
            Zeroizing::new(String::new()),
            || panic!("No reuse for an unconnected account")
        )
        .is_err()
    );
    assert!(connection_token(&cfg, Zeroizing::new(String::new()), || Ok(None)).is_err());
}

#[test]
fn credentials_changed_retry_deferred_threads_immediately() {
    let (_dir, mut store, cfg) = database();
    let task = ThreadTask {
        id: "thread".into(),
        repository: cfg.repositories[0].clone(),
        subject_type: "Issue".into(),
        title: "Test".into(),
        subject_url: "https://api.github.com/repos/example/private/issues/1".into(),
        latest_comment_url: None,
        reason: "mention".into(),
        since: Utc::now(),
        quiet: false,
    };
    store.enqueue(&cfg.login, &task).unwrap();
    store.retry_task(&task.id).unwrap();
    assert!(store.tasks(24).unwrap().is_empty());
    store.retry_pending_tasks().unwrap();
    assert_eq!(store.tasks(24).unwrap()[0].id, task.id);
}
