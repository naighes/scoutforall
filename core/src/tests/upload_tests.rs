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
        shapes::r#match::MatchEntry,
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
    /// where `PUT` takes it. It fails while `down`.
    struct FakeApi {
        url: String,
        down: Arc<AtomicBool>,
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
                requests: Arc::new(Mutex::new(vec![])),
                archives: Arc::new(Mutex::new(vec![])),
            };
            let (down, requests, archives) =
                (api.down.clone(), api.requests.clone(), api.archives.clone());
            tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    let (down, requests, archives) =
                        (down.clone(), requests.clone(), archives.clone());
                    tokio::spawn(async move {
                        answer(stream, address, &down, &requests, &archives).await;
                    });
                }
            });
            api
        }

        fn requests(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }
    }

    async fn answer(
        mut stream: TcpStream,
        address: std::net::SocketAddr,
        down: &AtomicBool,
        requests: &Mutex<Vec<String>>,
        archives: &Mutex<Vec<Vec<u8>>>,
    ) {
        let (head, body) = read_request(&mut stream).await;
        let line = head.lines().next().unwrap_or_default().to_string();
        let mut parts = line.split(' ');
        let (method, path) = (parts.next().unwrap(), parts.next().unwrap().to_string());
        requests
            .lock()
            .unwrap()
            .push(format!("{method} {}", path.split('?').next().unwrap()));
        let response = match (down.load(Ordering::SeqCst), method) {
            (true, _) => "HTTP/1.1 503 Service Unavailable\r\n".to_string(),
            (false, "POST") => {
                let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
                let file = request["filename"].as_str().unwrap();
                format!(
                    "HTTP/1.1 201 Created\r\nLocation: http://{address}/put/{file}?token=secret\r\n"
                )
            }
            (false, _) => {
                archives.lock().unwrap().push(body);
                "HTTP/1.1 200 OK\r\n".to_string()
            }
        };
        let response = format!("{response}Content-Length: 0\r\nConnection: close\r\n\r\n");
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
}
