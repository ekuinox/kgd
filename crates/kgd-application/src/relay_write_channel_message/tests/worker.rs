//! 転記ワーカーの直列処理のテスト。

use tokio::sync::mpsc;

use crate::test_support::{entry, text_sync_service, utc};

use super::*;

#[tokio::test]
async fn worker_processes_events_in_arrival_order() {
    use mockall::Sequence;

    let today = utc(2025, 1, 2, 0, 0);
    let mut repo = MockDiaryRepository::new();
    repo.expect_get_by_date()
        .times(2)
        .returning(move |_| Ok(Some(entry(100, today))));
    repo.expect_upsert_relayed_message()
        .times(2)
        .returning(|_| Ok(()));
    let mut gateway = MockDiscordGateway::new();
    // 転記が到着順（メッセージ 1 → 2）に行われることを Sequence で検証する
    let mut seq = Sequence::new();
    gateway
        .expect_send_text()
        .withf(|_, content| content.contains("https://discord.com/channels/1/500/1"))
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _| Ok(901));
    gateway
        .expect_send_text()
        .withf(|_, content| content.contains("https://discord.com/channels/1/500/2"))
        .times(1)
        .in_sequence(&mut seq)
        .returning(|_, _| Ok(902));
    gateway
        .expect_add_reaction()
        .times(2)
        .returning(|_, _, _| Ok(()));

    let relay = Arc::new(relay_use_case(repo, gateway, text_sync_service(2)));

    let (tx, rx) = mpsc::channel(8);
    let worker = tokio::spawn(run_relay_worker(relay, rx));

    tx.send(WriteChannelEvent::Posted(message(1, "first")))
        .await
        .unwrap();
    tx.send(WriteChannelEvent::Posted(message(2, "second")))
        .await
        .unwrap();

    // 送信側を閉じるとワーカーは残りを処理してから終了する
    drop(tx);
    worker.await.unwrap();
}
