use macaddr::MacAddr6;

use super::*;

#[test]
fn present_wake_outcome_formats_sent_message() {
    let outcome = WakeOutcome::Sent {
        name: "nas".to_string(),
        mac_address: MacAddr6::new(0, 0x11, 0x22, 0x33, 0x44, 0x55),
    };
    assert_eq!(
        present_wake_outcome(&outcome),
        Some("Sent WOL packet to nas (00:11:22:33:44:55)".to_string())
    );
}

#[test]
fn present_wake_outcome_returns_none_for_not_found() {
    assert_eq!(present_wake_outcome(&WakeOutcome::ServerNotFound), None);
}

#[test]
fn present_servers_builds_fields_and_footer() {
    let servers = vec![ServerTarget {
        name: "nas".to_string(),
        mac_address: MacAddr6::new(0, 0x11, 0x22, 0x33, 0x44, 0x55),
        ip_address: "192.168.1.10".to_string(),
        description: "storage".to_string(),
    }];
    let spec = present_servers(&servers);

    assert_eq!(spec.title, "Configured Servers");
    assert_eq!(spec.fields.len(), 1);
    assert_eq!(spec.fields[0].name, "nas");
    assert!(spec.fields[0].value.contains("192.168.1.10"));
    assert!(spec.fields[0].value.contains("storage"));
    assert_eq!(spec.footer, Some("Total: 1 server(s)".to_string()));
}

#[test]
fn present_server_status_marks_online_and_offline() {
    let statuses = vec![
        ServerStatus {
            name: "a".to_string(),
            online: true,
        },
        ServerStatus {
            name: "b".to_string(),
            online: false,
        },
    ];
    let spec = present_server_status(&statuses, Duration::from_secs(300));

    assert_eq!(spec.fields[0].value, "Online");
    assert_eq!(spec.fields[1].value, "Offline");
    assert_eq!(spec.footer, Some("Updated every 5m".to_string()));
}

#[test]
fn present_diary_create_outcome_formats_each_variant() {
    assert_eq!(
        present_diary_create_outcome(&DiaryCreateOutcome::Reopened { thread_id: 1 }),
        "今日の日報を再開しました: <#1>"
    );
    assert_eq!(
        present_diary_create_outcome(&DiaryCreateOutcome::ExistsButNotReopened { thread_id: 2 }),
        "今日の日報は既にありますが、再開はできません: <#2>"
    );
    let created = present_diary_create_outcome(&DiaryCreateOutcome::Created {
        thread_id: 3,
        page_url: "https://notion.example/x".to_string(),
        reused_page: false,
    });
    assert!(created.contains("日報を作成しました"));
    assert!(created.contains("<#3>"));
    assert!(created.contains("https://notion.example/x"));
}

#[test]
fn present_close_and_new_precheck_formats_messages() {
    assert_eq!(
        present_close_and_new_precheck(&CloseAndNewPrecheck::AlreadyLatest),
        Some("このスレッドが今日の最新の日報です".to_string())
    );
    assert_eq!(
        present_close_and_new_precheck(&CloseAndNewPrecheck::LatestExists { thread_id: 5 }),
        Some("今日の最新の日報はこちらです: <#5>".to_string())
    );
    assert_eq!(
        present_close_and_new_precheck(&CloseAndNewPrecheck::ReadyToCreate),
        None
    );
}
