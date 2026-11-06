//! Uploading the statistics of a match through the API: a local server
//! stands for it, signing the upload and taking the archive.

#[cfg(test)]
mod tests {
    use crate::{
        analytics::{
            queue_manager::QueueManager,
            upload::{UploadStep, upload_next},
        },
        archive::{import_match, import_team},
        localization::Language,
        providers::store::{
            match_reader::StoreMatchReader, match_writer::StoreMatchWriter,
            queue_reader::StoreQueueReader, queue_writer::StoreQueueWriter,
            set_reader::StoreSetReader, set_writer::StoreSetWriter, team_reader::StoreTeamReader,
            team_writer::StoreTeamWriter,
        },
        providers::{
            match_reader::MatchReader,
            match_writer::MatchWriter,
            team_reader::TeamReader,
            team_writer::{TeamInput, TeamWriter},
        },
        shapes::{
            country::CountryCode,
            enums::GenderEnum,
            r#match::{MatchEntry, MatchSource},
            team::TeamEntry,
        },
        storage::{Store, layout, memory::MemoryStore},
        tests::EN,
    };
    use std::{
        io::Cursor,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
        },
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };
    use uuid::Uuid;

    const TEAM_ID: &str = "45a589d5-004a-4a50-99f5-6d78fd052a3c";

    /// The API: `POST /sign` answers with the signed URL of the archive,
    /// where `PUT` takes it, and the keys of the match when asked, unless
    /// `old` (before v0.12). It fails while `down`.
    struct FakeApi {
        url: String,
        down: Arc<AtomicBool>,
        old: Arc<AtomicBool>,
        /// the bodies of the requests of a signed URL
        signs: Arc<Mutex<Vec<serde_json::Value>>>,
        /// the method and the path of each request, and the archives put
        requests: Arc<Mutex<Vec<String>>>,
        archives: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    impl FakeApi {
        async fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let api = FakeApi {
                url: format!("http://{address}/sign"),
                down: Arc::new(AtomicBool::new(false)),
                old: Arc::new(AtomicBool::new(false)),
                signs: Arc::new(Mutex::new(vec![])),
                requests: Arc::new(Mutex::new(vec![])),
                archives: Arc::new(Mutex::new(vec![])),
            };
            let api_state = ApiState {
                down: api.down.clone(),
                old: api.old.clone(),
                signs: api.signs.clone(),
                requests: api.requests.clone(),
                archives: api.archives.clone(),
            };
            tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    let state = api_state.clone();
                    tokio::spawn(async move {
                        answer(stream, address, &state).await;
                    });
                }
            });
            api
        }

        fn requests(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }
    }

    #[derive(Clone)]
    struct ApiState {
        down: Arc<AtomicBool>,
        old: Arc<AtomicBool>,
        signs: Arc<Mutex<Vec<serde_json::Value>>>,
        requests: Arc<Mutex<Vec<String>>>,
        archives: Arc<Mutex<Vec<Vec<u8>>>>,
    }

    /// The keys the API gives for a match: its date in each of them.
    fn keys_of(request: &serde_json::Value) -> serde_json::Value {
        let date = request["match"]["date"].as_str().unwrap();
        serde_json::json!({
            "team_key": format!("team-{date}"),
            "opponent_key": format!("opponent-{date}"),
            "match_key": format!("match-{date}"),
        })
    }

    async fn answer(mut stream: TcpStream, address: std::net::SocketAddr, state: &ApiState) {
        let ApiState {
            down,
            old,
            signs,
            requests,
            archives,
        } = state;
        let (head, body) = read_request(&mut stream).await;
        let line = head.lines().next().unwrap_or_default().to_string();
        let mut parts = line.split(' ');
        let (method, path) = (parts.next().unwrap(), parts.next().unwrap().to_string());
        requests
            .lock()
            .unwrap()
            .push(format!("{method} {}", path.split('?').next().unwrap()));
        let (response, content) = match (down.load(Ordering::SeqCst), method) {
            (true, _) => (
                "HTTP/1.1 503 Service Unavailable\r\n".to_string(),
                String::new(),
            ),
            (false, "POST") => {
                let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                signs.lock().unwrap().push(request.clone());
                let file = request["filename"].as_str().unwrap();
                let keys = match (old.load(Ordering::SeqCst), request.get("match")) {
                    (false, Some(_)) => keys_of(&request).to_string(),
                    _ => String::new(),
                };
                (
                    format!(
                        "HTTP/1.1 201 Created\r\nLocation: http://{address}/put/{file}?token=secret\r\n"
                    ),
                    keys,
                )
            }
            (false, _) => {
                archives.lock().unwrap().push(body);
                ("HTTP/1.1 200 OK\r\n".to_string(), String::new())
            }
        };
        let response = format!(
            "{response}Content-Length: {}\r\nConnection: close\r\n\r\n{content}",
            content.len()
        );
        stream.write_all(response.as_bytes()).await.unwrap();
    }

    /// The head of the request and its body, as long as it says.
    async fn read_request(stream: &mut TcpStream) -> (String, Vec<u8>) {
        let mut data = vec![];
        let mut buffer = [0u8; 4096];
        loop {
            let n = stream.read(&mut buffer).await.unwrap();
            data.extend_from_slice(&buffer[..n]);
            let text = String::from_utf8_lossy(&data).to_string();
            if let Some(end) = text.find("\r\n\r\n") {
                let head = text[..end].to_string();
                let length = head
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                let start = end + 4;
                if data.len() >= start + length || n == 0 {
                    return (head, data[start..].to_vec());
                }
            }
            if n == 0 {
                return (String::from_utf8_lossy(&data).to_string(), vec![]);
            }
        }
    }

    struct App {
        store: Arc<dyn Store>,
        team_reader: StoreTeamReader,
        match_reader: StoreMatchReader,
        queue: QueueManager<StoreQueueReader, StoreQueueWriter>,
    }

    impl App {
        /// The team and the match of the archive fixtures, the match queued.
        async fn with_a_finished_match() -> (Self, MatchEntry) {
            let store: Arc<dyn Store> = Arc::new(MemoryStore::new());
            let language = Language::default;
            let app = App {
                team_reader: StoreTeamReader::new(store.clone(), language()),
                match_reader: StoreMatchReader::new(
                    store.clone(),
                    Arc::new(StoreSetReader::new(store.clone(), language())),
                    language(),
                ),
                queue: QueueManager::new(
                    Arc::new(StoreQueueReader::new(store.clone())),
                    Arc::new(StoreQueueWriter::new(store.clone())),
                ),
                store: store.clone(),
            };
            let team = import_team(
                include_bytes!("fixtures/45a589d5-004a-4a50-99f5-6d78fd052a3c.zip"),
                Uuid::parse_str(TEAM_ID).ok(),
                &app.team_reader,
                &StoreTeamWriter::new(store.clone()),
                EN,
            )
            .await
            .unwrap();
            let m = import_match(
                include_bytes!("fixtures/2025-09-20_club_arezzo.zip"),
                &team,
                &app.match_reader,
                &StoreMatchWriter::new(store.clone()),
                &StoreSetWriter::new(store.clone(), language()),
                EN,
            )
            .await
            .unwrap();
            app.queue.enqueue(team.id, m.id.clone()).await.unwrap();
            (app, m)
        }

        /// The team of the match with its gender and country, as since
        /// v0.12.
        async fn with_gender_and_country(&self, m: &MatchEntry) -> MatchEntry {
            let team = TeamEntry {
                gender: Some(GenderEnum::Women),
                country: CountryCode::parse("IT"),
                ..self.team_reader.read_single(&m.team.id).await.unwrap()
            };
            StoreTeamWriter::new(self.store.clone())
                .save(TeamInput::Existing(team.clone()))
                .await
                .unwrap();
            self.match_reader.read_single(&team, &m.id).await.unwrap()
        }

        /// The `match.json` of the archive put.
        fn uploaded_match(api: &FakeApi, index: usize) -> serde_json::Value {
            let archive = api.archives.lock().unwrap()[index].clone();
            let mut zip = zip::ZipArchive::new(Cursor::new(archive)).unwrap();
            let content = std::io::read_to_string(zip.by_name("match.json").unwrap()).unwrap();
            serde_json::from_str(&content).unwrap()
        }

        async fn upload_next(&self, api: &FakeApi) -> UploadStep {
            upload_next(
                &*self.store,
                &self.team_reader,
                &self.match_reader,
                &self.queue,
                &api.url,
            )
            .await
            .unwrap()
        }

        async fn queued(&self) -> Vec<(String, u8)> {
            let queue = self.queue.load().await.unwrap();
            queue
                .peek()
                .map(|p| (p.match_id.clone(), p.retry_count))
                .into_iter()
                .collect()
        }
    }

    #[tokio::test]
    async fn the_match_is_signed_then_put_and_leaves_the_queue() {
        let api = FakeApi::start().await;
        let (app, m) = App::with_a_finished_match().await;
        assert_eq!(app.upload_next(&api).await, UploadStep::Uploaded);
        assert_eq!(app.queued().await, vec![]);
        let requests = api.requests();
        assert_eq!(requests[0], "POST /sign");
        assert!(requests[1].starts_with("PUT /put/"), "{requests:?}");
        let archive = api.archives.lock().unwrap()[0].clone();
        let mut zip = zip::ZipArchive::new(Cursor::new(archive)).unwrap();
        let mut names: Vec<String> = zip.file_names().map(|n| n.unwrap().to_string()).collect();
        names.sort();
        assert_eq!(
            names,
            ["match.json", "set_1_descriptor.json", "set_1_events.csv"]
        );
        let events = std::io::read_to_string(zip.by_name("set_1_events.csv").unwrap()).unwrap();
        let saved = app
            .store
            .read(&layout::set_events(&m.team.id, &m.id, 1))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(events.as_bytes(), saved);
        // nothing else
        assert_eq!(app.upload_next(&api).await, UploadStep::Empty);
    }

    /// No network, or the API down: the match stays, to be sent later.
    #[tokio::test]
    async fn a_failed_upload_stays_in_the_queue_until_sent() {
        let api = FakeApi::start().await;
        api.down.store(true, Ordering::SeqCst);
        let (app, m) = App::with_a_finished_match().await;
        assert_eq!(app.upload_next(&api).await, UploadStep::Failed);
        assert_eq!(app.queued().await, vec![(m.id.clone(), 1)]);
        api.down.store(false, Ordering::SeqCst);
        assert_eq!(app.upload_next(&api).await, UploadStep::Uploaded);
        assert_eq!(app.queued().await, vec![]);
    }

    #[tokio::test]
    async fn the_same_match_goes_to_the_same_file() {
        let api = FakeApi::start().await;
        let (app, m) = App::with_a_finished_match().await;
        app.upload_next(&api).await;
        // e.g. the queue of a backup restored on a new phone
        app.queue.enqueue(m.team.id, m.id.clone()).await.unwrap();
        app.upload_next(&api).await;
        let puts: Vec<String> = api
            .requests()
            .into_iter()
            .filter(|r| r.starts_with("PUT"))
            .collect();
        assert_eq!(puts.len(), 2);
        assert_eq!(puts[0], puts[1]);
    }

    #[tokio::test]
    async fn a_match_no_longer_there_leaves_the_queue() {
        let api = FakeApi::start().await;
        let (app, m) = App::with_a_finished_match().await;
        app.queue
            .enqueue(m.team.id, "2025-01-01_removed".into())
            .await
            .unwrap();
        app.queue.dequeue(&m.team.id, &m.id).await.unwrap();
        assert_eq!(app.upload_next(&api).await, UploadStep::Dropped);
        assert_eq!(app.queued().await, vec![]);
        assert_eq!(api.requests(), Vec::<String>::new());
    }

    #[tokio::test]
    async fn the_keys_of_the_match_are_asked_and_written_in_the_archive() {
        let api = FakeApi::start().await;
        let (app, m) = App::with_a_finished_match().await;
        let m = app.with_gender_and_country(&m).await;
        assert_eq!(app.upload_next(&api).await, UploadStep::Uploaded);
        let sign = api.signs.lock().unwrap()[0].clone();
        let date = m.date.format("%Y-%m-%d").to_string();
        let sets: Vec<serde_json::Value> = m
            .set_scores()
            .unwrap()
            .into_iter()
            .map(|(us, them)| serde_json::json!([us, them]))
            .collect();
        assert!(!sets.is_empty());
        assert_eq!(
            sign["match"],
            serde_json::json!({
                "team": {
                    "name": "Volley Club",
                    "country": "IT",
                    "gender": "women",
                    "year": m.team.year,
                },
                "opponent": { "name": m.opponent },
                "date": date,
                "sets": sets,
            })
        );
        let uploaded = App::uploaded_match(&api, 0);
        assert_eq!(uploaded["team_key"], format!("team-{date}"));
        assert_eq!(uploaded["opponent_key"], format!("opponent-{date}"));
        assert_eq!(uploaded["match_key"], format!("match-{date}"));
        assert_eq!(uploaded["source"], "scout4all");
        // the names go to the API only, never in the archive
        let text = uploaded.to_string();
        assert!(!text.contains("Volley Club"), "{text}");
        assert!(!text.contains(&m.opponent), "{text}");
    }

    /// A team saved before v0.12, without gender and country: no keys, as
    /// before.
    #[tokio::test]
    async fn a_team_without_country_asks_no_keys() {
        let api = FakeApi::start().await;
        let (app, _) = App::with_a_finished_match().await;
        assert_eq!(app.upload_next(&api).await, UploadStep::Uploaded);
        let sign = api.signs.lock().unwrap()[0].clone();
        assert_eq!(sign.get("match"), None, "{sign}");
        let uploaded = App::uploaded_match(&api, 0);
        assert_eq!(uploaded.get("team_key"), None, "{uploaded}");
        assert_eq!(uploaded["source"], "scout4all");
    }

    /// The API not updated yet: the match goes without its keys.
    #[tokio::test]
    async fn an_api_without_keys_takes_the_match_anyway() {
        let api = FakeApi::start().await;
        api.old.store(true, Ordering::SeqCst);
        let (app, m) = App::with_a_finished_match().await;
        app.with_gender_and_country(&m).await;
        assert_eq!(app.upload_next(&api).await, UploadStep::Uploaded);
        assert!(api.signs.lock().unwrap()[0].get("match").is_some());
        let uploaded = App::uploaded_match(&api, 0);
        assert_eq!(uploaded.get("match_key"), None, "{uploaded}");
        assert_eq!(app.queued().await, vec![]);
    }

    #[tokio::test]
    async fn an_imported_match_is_uploaded_as_such() {
        let api = FakeApi::start().await;
        let (app, m) = App::with_a_finished_match().await;
        StoreMatchWriter::new(app.store.clone())
            .save_source(&m, MatchSource::Dvw, false)
            .await
            .unwrap();
        assert_eq!(app.upload_next(&api).await, UploadStep::Uploaded);
        assert_eq!(App::uploaded_match(&api, 0)["source"], "dvw");
    }
}
