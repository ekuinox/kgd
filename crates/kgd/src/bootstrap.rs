//! 各層の実装を組み立てて Bot を起動する Composition Root。

use std::{sync::Arc, time::Duration};

use anyhow::{Context as _, Result};
use serenity::{
    all::{ChannelId, GatewayIntents, Http},
    prelude::*,
};
use tokio::sync::mpsc;
use tracing::info;

use kgd_application::{
    AutoCloseJob, DiaryLifecycleSettings, DiaryMaintenanceSettings, HourlySyncJob,
    ManageDiaryLifecycle, RelaySettings, RelayWriteChannelMessage, RunDiaryMaintenance,
    SyncDiaryMessage, WakeServer,
    ports::{
        AttachmentDownloader, Clock, DiaryRepository, DiscordGateway, ImageConverter, NotionApi,
        OgpClient, WolSender,
    },
};
use kgd_domain::{ServerStatus, compile_url_rules};
use kgd_infrastructure::{
    DiaryStore, HeifConverter, NotionClient, OgpFetcher, ReqwestDownloader, Scheduler,
    SerenityGateway, SystemClock, UdpWolSender,
};
use kgd_presentation::{
    Handler, HandlerSettings, StatusNotifier, VersionInfo, run_status_receiver,
};

use crate::{config::Config, version};

/// Discord Bot を起動し、イベントループを開始する。
pub async fn run(config: Config, status_rx: mpsc::Receiver<Vec<ServerStatus>>) -> Result<()> {
    let mut intents = GatewayIntents::GUILDS;

    // メッセージイベントを購読
    intents |= GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT;

    let diary_config = &config.diary;

    // 起動時に URL ルールのバリデーションとコンパイルを行う
    let url_rules = compile_url_rules(&diary_config.url_rules, &diary_config.default_convert_to)
        .context("Invalid URL rules in configuration")?;

    let diary_store: Arc<dyn DiaryRepository> = Arc::new(
        DiaryStore::connect(&diary_config.database_url)
            .await
            .context("Failed to connect to database")?,
    );
    let notion_client: Arc<dyn NotionApi> = Arc::new(
        NotionClient::new(
            &diary_config.notion_token,
            &diary_config.notion_database_id,
            &diary_config.notion_title_property,
            diary_config.notion_tags.clone(),
        )
        .context("Failed to create Notion client")?,
    );

    // メッセージ同期ユースケースをポート実装と組み立てる
    let ogp_client = if diary_config.ogp_enabled {
        let fetcher =
            OgpFetcher::new(diary_config.ogp_timeout).context("Failed to create OGP fetcher")?;
        Some(Arc::new(fetcher) as Arc<dyn OgpClient>)
    } else {
        None
    };
    let sync_service = Arc::new(SyncDiaryMessage::new(
        notion_client.clone(),
        diary_store.clone(),
        Arc::new(ReqwestDownloader::new()) as Arc<dyn AttachmentDownloader>,
        Arc::new(HeifConverter) as Arc<dyn ImageConverter>,
        ogp_client,
        url_rules,
    ));

    // 定期メンテナンス・ライフサイクルユースケースを組み立てる。
    // serenity の Client 構築前にゲートウェイが必要なため、REST 専用の Http を別途作る。
    let gateway: Arc<dyn DiscordGateway> = Arc::new(SerenityGateway::new(Arc::new(Http::new(
        &config.discord.token,
    ))));
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let maintenance = Arc::new(RunDiaryMaintenance::new(
        diary_store.clone(),
        gateway.clone(),
        clock.clone(),
        sync_service.clone(),
        DiaryMaintenanceSettings {
            timezone: diary_config.timezone,
            auto_close_enabled: diary_config.auto_close_enabled,
            auto_close_hour: diary_config.auto_close_hour,
            write_channel_id: diary_config.write_channel_id,
            sync_reaction: diary_config.sync_reaction.clone(),
        },
    ));
    let lifecycle = Arc::new(ManageDiaryLifecycle::new(
        diary_store.clone(),
        notion_client,
        gateway.clone(),
        clock.clone(),
        DiaryLifecycleSettings {
            timezone: diary_config.timezone,
            forum_channel_id: diary_config.forum_channel_id,
            write_channel_id: diary_config.write_channel_id,
        },
    ));
    let relay = Arc::new(RelayWriteChannelMessage::new(
        diary_store.clone(),
        gateway,
        clock,
        sync_service.clone(),
        RelaySettings {
            timezone: diary_config.timezone,
            sync_reaction: diary_config.sync_reaction.clone(),
        },
    ));

    let servers = config.server_targets();
    let wake_server = Arc::new(WakeServer::new(
        Arc::new(UdpWolSender) as Arc<dyn WolSender>,
        servers.clone(),
    ));

    let handler = Handler::new(
        HandlerSettings {
            admins: config.discord.admins.clone(),
            version_info: VersionInfo {
                version: version::VERSION.to_string(),
                git_sha: version::GIT_SHA.to_string(),
                target_triple: version::TARGET_TRIPLE.to_string(),
                build_date: version::BUILD_DATE.to_string(),
            },
            servers,
            write_channel_id: diary_config.write_channel_id,
        },
        diary_store,
        sync_service,
        maintenance.clone(),
        lifecycle,
        relay,
        wake_server,
    );

    let mut client = Client::builder(&config.discord.token, intents)
        .event_handler(handler)
        .await
        .context("Failed to create Discord client")?;

    let notifier = StatusNotifier::new(
        client.http.clone(),
        ChannelId::new(config.discord.status_channel_id),
        config.status.interval,
    );

    tokio::spawn(run_status_receiver(notifier, status_rx));

    // 日報向けの定期ジョブをスケジューラに登録して起動
    let diary_interval = Duration::from_secs(60);
    let mut scheduler = Scheduler::new(diary_interval);
    scheduler.register(Arc::new(AutoCloseJob(maintenance.clone())));
    scheduler.register(Arc::new(HourlySyncJob(maintenance)));
    tokio::spawn(scheduler.run());
    info!(interval = ?diary_interval, "Diary periodic tasks started");

    info!("Starting bot");
    client.start().await.context("Discord client error")?;

    Ok(())
}
