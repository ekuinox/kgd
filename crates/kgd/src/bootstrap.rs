//! 各層の実装を組み立てて Bot を起動する Composition Root。

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context as _, Result};
use serenity::{
    all::{ChannelId, GatewayIntents, Http},
    prelude::*,
};
use tokio::sync::mpsc;
use tracing::{info, warn};

use kgd_application::{
    AutoCloseJob, BrowseLocationHistoryUseCase, BuildLocationReportUseCase, DailyLocationReportJob,
    DiaryLifecycleSettings, DiaryMaintenanceSettings, HourlySyncJob, LocationHistorySettings,
    LocationReportSettings, ManageDiaryLifecycleUseCase, PublishDiaryPostUseCase,
    RecordLocationUseCase, RelaySettings, RelayWriteChannelMessageUseCase,
    RunDiaryMaintenanceUseCase, SyncDiaryMessageUseCase, WakeServerUseCase,
    ports::{
        AttachmentDownloader, Clock, DiaryPostRepository, DiaryRepository, DiscordGateway,
        ImageConverter, LocationRepository, MapRenderer, NotionApi, OgpClient, WolSender,
    },
    run_relay_worker,
};
use kgd_domain::{DiaryCalendar, ServerStatus, compile_url_rules};
use kgd_infrastructure::{
    DiaryPostStore, DiaryStore, HeifConverter, LocationStore, NotionClient, OgpFetcher,
    ReqwestDownloader, Scheduler, SerenityGateway, SystemClock, TileMapRenderer, UdpWolSender,
    bind_http, connect_pool, serve_http,
};
use kgd_presentation::{
    DiscordController, DiscordControllerSettings, LocationReportCommand,
    OwnTracksControllerSettings, StatusNotifier, VersionInfo, ViewerSettings, owntracks_router,
    run_status_receiver, viewer_router,
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

    let pool = connect_pool(&diary_config.database_url)
        .await
        .context("Failed to connect to database")?;
    let diary_store: Arc<dyn DiaryRepository> = Arc::new(DiaryStore::new(pool.clone()));
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
    let sync_service = Arc::new(SyncDiaryMessageUseCase::new(
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
    // 日報日の区切り方は全ユースケースで共有する
    let calendar = DiaryCalendar::new(diary_config.timezone, diary_config.day_start_hour);

    // 位置ログのレポート。`[location]` が無ければ作らない。
    // スラッシュコマンドと定時ジョブの両方が同じユースケースを使う。
    let mut location_report: Option<Arc<BuildLocationReportUseCase>> = None;
    let mut location_report_job: Option<Arc<DailyLocationReportJob>> = None;
    if let Some(location_config) = &config.location {
        let location_store: Arc<dyn LocationRepository> =
            Arc::new(LocationStore::new(pool.clone()));
        // OSM のタイル利用規約は識別可能な User-Agent を求める
        let user_agent = format!("kgd/{} (+https://github.com/ekuinox/kgd)", version::VERSION);
        let renderer: Arc<dyn MapRenderer> = Arc::new(
            TileMapRenderer::new(&user_agent, &location_config.tile_cache_dir)
                .context("Failed to create map renderer")?,
        );
        let build = Arc::new(BuildLocationReportUseCase::new(
            location_store,
            renderer,
            LocationReportSettings {
                calendar,
                max_accuracy_m: location_config.max_accuracy_m,
                image_width: location_config.image_width,
                image_height: location_config.image_height,
            },
        ));
        if location_config.daily_report_enabled {
            let publish = Arc::new(PublishDiaryPostUseCase::new(
                diary_store.clone(),
                Arc::new(DiaryPostStore::new(pool.clone())) as Arc<dyn DiaryPostRepository>,
                notion_client.clone(),
                gateway.clone(),
                clock.clone(),
                calendar,
            ));
            location_report_job = Some(Arc::new(DailyLocationReportJob::new(
                build.clone(),
                publish,
                clock.clone(),
                calendar,
            )));
        }
        location_report = Some(build);
    }

    let maintenance = Arc::new(RunDiaryMaintenanceUseCase::new(
        diary_store.clone(),
        gateway.clone(),
        clock.clone(),
        sync_service.clone(),
        DiaryMaintenanceSettings {
            calendar,
            auto_close_enabled: diary_config.auto_close_enabled,
            write_channel_id: diary_config.write_channel_id,
            sync_reaction: diary_config.sync_reaction.clone(),
        },
    ));
    let lifecycle = Arc::new(ManageDiaryLifecycleUseCase::new(
        diary_store.clone(),
        notion_client,
        gateway.clone(),
        clock.clone(),
        DiaryLifecycleSettings {
            calendar,
            forum_channel_id: diary_config.forum_channel_id,
            write_channel_id: diary_config.write_channel_id,
        },
    ));
    let relay = Arc::new(RelayWriteChannelMessageUseCase::new(
        diary_store.clone(),
        gateway,
        clock,
        sync_service.clone(),
        RelaySettings {
            calendar,
            sync_reaction: diary_config.sync_reaction.clone(),
        },
    ));

    // 書き込み用チャンネルの転記は順序を保つため単一ワーカーで直列に処理する
    let (relay_tx, relay_rx) = mpsc::channel(64);
    tokio::spawn(run_relay_worker(relay, relay_rx));

    let servers = config.server_targets();
    let wake_server = Arc::new(WakeServerUseCase::new(
        Arc::new(UdpWolSender) as Arc<dyn WolSender>,
        servers.clone(),
    ));

    let handler = DiscordController::new(
        DiscordControllerSettings {
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
        relay_tx,
        wake_server,
        location_report.map(|build| LocationReportCommand {
            build,
            calendar,
            clock: Arc::new(SystemClock) as Arc<dyn Clock>,
        }),
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
    if let Some(job) = location_report_job {
        scheduler.register(job);
    }
    tokio::spawn(scheduler.run());
    info!(interval = ?diary_interval, "Diary periodic tasks started");

    // 位置情報の受け口。設定が無ければ起動しない。
    // bind は起動処理内で同期的に行い、失敗を `?` で起動失敗として伝搬させる。
    // `[location]` を書くこと自体が受け口を動かす意思表示であり、他の起動時
    // 前提条件 (DB 接続や URL ルールの検証など) と同様に、ポート衝突などは
    // デーモン全体を落として気づけるようにする。
    if let Some(location_config) = config.location.clone() {
        let location_store: Arc<dyn LocationRepository> =
            Arc::new(LocationStore::new(pool.clone()));
        let record_location = Arc::new(RecordLocationUseCase::new(location_store.clone()));
        let mut router = owntracks_router(
            record_location,
            OwnTracksControllerSettings {
                username: location_config.username.clone(),
                password: location_config.password.clone(),
            },
        );
        // ビューアは `[location.viewer]` を書いたときだけ有効にする (ADR-0013)。
        if let Some(viewer_config) = &location_config.viewer {
            let loopbacks = [
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                IpAddr::V6(Ipv6Addr::LOCALHOST),
            ];
            if viewer_config
                .allowed_cidrs
                .iter()
                .any(|net| loopbacks.iter().any(|ip| net.contains(ip)))
            {
                warn!(
                    "location.viewer.allowed_cidrs includes loopback. Requests through a local \
                     cloudflared are then blocked only by the Cloudflare header check"
                );
            }
            let browse = Arc::new(BrowseLocationHistoryUseCase::new(
                location_store,
                LocationHistorySettings {
                    timezone: diary_config.timezone,
                    max_accuracy_m: location_config.max_accuracy_m,
                    max_track_points: viewer_config.max_track_points,
                },
            ));
            router = router.merge(viewer_router(
                browse,
                ViewerSettings {
                    allowed_cidrs: viewer_config.allowed_cidrs.clone(),
                },
            ));
            info!("Location viewer enabled at /viewer/");
        }
        let listener = bind_http(location_config.listen)
            .await
            .context("Failed to start OwnTracks HTTP receiver")?;
        tokio::spawn(async move {
            if let Err(error) = serve_http(listener, router).await {
                tracing::error!(?error, "OwnTracks HTTP server stopped");
            }
        });
    }

    info!("Starting bot");
    client.start().await.context("Discord client error")?;

    Ok(())
}
