//! ONVIF: the IP cameras on the network found (WS-Discovery, a probe sent
//! to everyone on each network of this computer), and the RTSP address of
//! their video asked for (`GetProfiles`, `GetStreamUri`) with the user and
//! the password of the camera (WS-Security, the password digested). A
//! camera found is kept as its ONVIF address (`http://…/onvif/…`): its
//! RTSP address asked again at each connection, as the camera says it.

use {
    super::network::{ANSWER_WITHIN, Problem, problem_of},
    base64::{Engine, engine::general_purpose::STANDARD as BASE64},
    chrono::{DateTime, TimeDelta, Utc},
    std::{
        net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
        time::{Duration, Instant},
    },
};

/// Where WS-Discovery is asked.
const DISCOVERY: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(239, 255, 255, 250), 3702);

/// How long the cameras are waited for.
pub const SEARCH_WITHIN: Duration = Duration::from_secs(3);

/// A camera that answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// as it calls itself (its name, else its model), else its address
    pub name: String,
    /// its ONVIF address: the one to connect to
    pub address: String,
}

impl Found {
    /// The host of its address, to say where it is.
    pub fn host(&self) -> String {
        url::Url::parse(&self.address)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_default()
    }
}

/// Whether an address is a camera's ONVIF one (as found, as written).
pub fn is_onvif(url: &url::Url) -> bool {
    matches!(url.scheme(), "http" | "https") && url.path().to_lowercase().contains("/onvif/")
}

/// The cameras on the networks of this computer that answer within
/// `within`: each once, in the order they answered.
pub fn discover(within: Duration) -> Vec<Found> {
    let sockets: Vec<UdpSocket> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            std::net::IpAddr::V4(ip) => probe_socket(ip).ok(),
            std::net::IpAddr::V6(_) => None,
        })
        .collect();
    let message = probe_message(&uuid::Uuid::new_v4().to_string());
    let sent: Vec<UdpSocket> = sockets
        .into_iter()
        .filter(|s| s.send_to(message.as_bytes(), DISCOVERY).is_ok())
        .collect();
    collect(&sent, within)
}

/// A socket on the network of `ip` that sends to everyone there.
fn probe_socket(ip: Ipv4Addr) -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_multicast_if_v4(&ip)?;
    socket.set_multicast_ttl_v4(2)?;
    socket.bind(&SocketAddr::from((ip, 0)).into())?;
    let socket: UdpSocket = socket.into();
    socket.set_read_timeout(Some(Duration::from_millis(100)))?;
    Ok(socket)
}

/// The answers read from the sockets until `within` is past.
fn collect(sockets: &[UdpSocket], within: Duration) -> Vec<Found> {
    let until = Instant::now() + within;
    let mut found: Vec<Found> = vec![];
    let mut buffer = vec![0u8; 65_535];
    while Instant::now() < until && !sockets.is_empty() {
        for socket in sockets {
            let Ok((read, _)) = socket.recv_from(&mut buffer) else {
                continue;
            };
            let answer = String::from_utf8_lossy(&buffer[..read]);
            for camera in matches(&answer) {
                if !found.iter().any(|f| f.address == camera.address) {
                    found.push(camera);
                }
            }
        }
    }
    found
}

/// The probe for the devices that send video (`NetworkVideoTransmitter`).
fn probe_message(id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><e:Envelope xmlns:e="http://www.w3.org/2003/05/soap-envelope" xmlns:w="http://schemas.xmlsoap.org/ws/2004/08/addressing" xmlns:d="http://schemas.xmlsoap.org/ws/2005/04/discovery" xmlns:dn="http://www.onvif.org/ver10/network/wsdl"><e:Header><w:MessageID>uuid:{id}</w:MessageID><w:To e:mustUnderstand="true">urn:schemas-xmlsoap-org:ws:2005:04:discovery</w:To><w:Action e:mustUnderstand="true">http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe</w:Action></e:Header><e:Body><d:Probe><d:Types>dn:NetworkVideoTransmitter</d:Types></d:Probe></e:Body></e:Envelope>"#
    )
}

/// The cameras in an answer to the probe: the address on IPv4 over
/// HTTP of each (the first, if it says more), its name from its scopes.
fn matches(answer: &str) -> Vec<Found> {
    elements(answer, "ProbeMatch")
        .into_iter()
        .filter_map(|(_, body)| {
            let addresses = first_text(body, "XAddrs")?;
            let address = addresses
                .split_whitespace()
                .find(|a| {
                    url::Url::parse(a).is_ok_and(|u| {
                        matches!(u.host(), Some(url::Host::Ipv4(_) | url::Host::Domain(_)))
                    })
                })
                .or_else(|| addresses.split_whitespace().next())?
                .to_string();
            let scopes = first_text(body, "Scopes").unwrap_or_default();
            let scope = |what: &str| {
                scopes
                    .split_whitespace()
                    .find_map(|s| s.strip_prefix(&format!("onvif://www.onvif.org/{what}/")))
                    .map(|v| percent_decoded(v).replace('_', " "))
                    .filter(|v| !v.trim().is_empty())
            };
            let host = url::Url::parse(&address)
                .ok()
                .and_then(|u| u.host_str().map(str::to_string))
                .unwrap_or_default();
            let name = scope("name")
                .or_else(|| scope("hardware"))
                .unwrap_or_else(|| host.clone());
            Some(Found { name, address })
        })
        .collect()
}

fn percent_decoded(text: &str) -> String {
    url::form_urlencoded::parse(format!("v={}", text.replace('+', "%2B")).as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_else(|| text.to_string())
}

/// The RTSP address of the camera's video: of its first profile in H.264
/// (else its first), asked of its media service with `user`.
pub async fn stream_uri(
    device: &url::Url,
    user: Option<(String, String)>,
) -> Result<url::Url, Problem> {
    let client = reqwest::Client::builder()
        .timeout(ANSWER_WITHIN)
        .build()
        .map_err(|e| Problem::Other(e.to_string()))?;
    let soap = Soap {
        client,
        user,
        // the camera's clock, for the digest: it refuses one too far off
        offset: camera_clock(&Soap::anonymous(), device).await,
    };
    let capabilities = soap
        .call(
            device,
            r#"<GetCapabilities xmlns="http://www.onvif.org/ver10/device/wsdl"><Category>Media</Category></GetCapabilities>"#,
        )
        .await?;
    // the media service where it says, else where the device is
    let media = elements(&capabilities, "Media")
        .into_iter()
        .find_map(|(_, body)| first_text(body, "XAddr"))
        .and_then(|a| url::Url::parse(&a).ok())
        .map(|u| same_host(u, device))
        .unwrap_or_else(|| device.clone());
    let profiles = soap
        .call(
            &media,
            r#"<GetProfiles xmlns="http://www.onvif.org/ver10/media/wsdl"/>"#,
        )
        .await?;
    let token = profile_token(&profiles).ok_or_else(|| Problem::NoVideo(String::new()))?;
    let uri = soap
        .call(
            &media,
            &format!(
                r#"<GetStreamUri xmlns="http://www.onvif.org/ver10/media/wsdl"><StreamSetup><Stream xmlns="http://www.onvif.org/ver10/schema">RTP-Unicast</Stream><Transport xmlns="http://www.onvif.org/ver10/schema"><Protocol>RTSP</Protocol></Transport></StreamSetup><ProfileToken>{}</ProfileToken></GetStreamUri>"#,
                escaped(&token)
            ),
        )
        .await?;
    let uri = elements(&uri, "MediaUri")
        .into_iter()
        .find_map(|(_, body)| first_text(body, "Uri"))
        .ok_or_else(|| Problem::NoVideo(String::new()))?;
    url::Url::parse(&uri)
        .map(|u| same_host(u, device))
        .map_err(|_| Problem::Other(uri))
}

/// An address the camera gives of itself, on the host it was reached at
/// when it says one that isn't (`0.0.0.0`, `localhost`: behind a router, or
/// not set up).
fn same_host(mut given: url::Url, reached: &url::Url) -> url::Url {
    let unusable_ip = |ip: std::net::IpAddr| ip.is_unspecified() || ip.is_loopback();
    // `rtsp://` isn't a scheme `url` knows: its IP is a name to it
    let unusable = match given.host() {
        Some(url::Host::Ipv4(ip)) => unusable_ip(ip.into()),
        Some(url::Host::Ipv6(ip)) => unusable_ip(ip.into()),
        Some(url::Host::Domain(d)) => {
            d.eq_ignore_ascii_case("localhost")
                || d.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(unusable_ip)
        }
        None => true,
    };
    if unusable && let Some(host) = reached.host_str() {
        let _ = given.set_host(Some(host));
    }
    given
}

/// The token of the first profile in H.264, else of the first.
fn profile_token(profiles: &str) -> Option<String> {
    let all = elements(profiles, "Profiles");
    let h264 = all.iter().find(|(_, body)| {
        first_text(body, "Encoding").is_some_and(|e| e.eq_ignore_ascii_case("H264"))
    });
    h264.or(all.first())
        .and_then(|(attributes, _)| attribute(attributes, "token"))
}

/// How far the camera's clock is from this one's, as it says it
/// (`GetSystemDateAndTime`, asked of everyone); none, none.
async fn camera_clock(soap: &Soap, device: &url::Url) -> TimeDelta {
    let answer = soap
        .call(
            device,
            r#"<GetSystemDateAndTime xmlns="http://www.onvif.org/ver10/device/wsdl"/>"#,
        )
        .await;
    answer
        .ok()
        .and_then(|a| utc_of(&a))
        .map(|theirs| theirs - Utc::now())
        .unwrap_or_default()
}

/// The time in UTC in the answer to `GetSystemDateAndTime`.
fn utc_of(answer: &str) -> Option<DateTime<Utc>> {
    let (_, utc) = elements(answer, "UTCDateTime").into_iter().next()?;
    let number = |body: &str, name: &str| first_text(body, name)?.trim().parse::<u32>().ok();
    let (_, date) = elements(utc, "Date").into_iter().next()?;
    let (_, time) = elements(utc, "Time").into_iter().next()?;
    let date = chrono::NaiveDate::from_ymd_opt(
        i32::try_from(number(date, "Year")?).ok()?,
        number(date, "Month")?,
        number(date, "Day")?,
    )?;
    let time = chrono::NaiveTime::from_hms_opt(
        number(time, "Hour")?,
        number(time, "Minute")?,
        number(time, "Second")?,
    )?;
    Some(date.and_time(time).and_utc())
}

/// The requests to the camera: SOAP 1.2, with the user if any.
struct Soap {
    client: reqwest::Client,
    user: Option<(String, String)>,
    offset: TimeDelta,
}

impl Soap {
    fn anonymous() -> Soap {
        Soap {
            client: reqwest::Client::builder()
                .timeout(ANSWER_WITHIN)
                .build()
                .unwrap_or_default(),
            user: None,
            offset: TimeDelta::zero(),
        }
    }

    /// The body of the answer to `body`; a fault as a problem.
    async fn call(&self, to: &url::Url, body: &str) -> Result<String, Problem> {
        let header = match &self.user {
            Some((name, password)) if !name.is_empty() => security(
                name,
                password,
                uuid::Uuid::new_v4().as_bytes(),
                Utc::now() + self.offset,
            ),
            _ => String::new(),
        };
        let envelope = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?><s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope"><s:Header>{header}</s:Header><s:Body>{body}</s:Body></s:Envelope>"#
        );
        let response = self
            .client
            .post(to.clone())
            .header("Content-Type", "application/soap+xml; charset=utf-8")
            .body(envelope)
            .send()
            .await
            .map_err(|e| match e.is_timeout() {
                true => Problem::Silent,
                false => problem_of(&format!("unable to connect: {e}")),
            })?;
        let status = response.status();
        let text = response
            .text()
            .await
            .map_err(|e| problem_of(&e.to_string()))?;
        match (status.as_u16(), fault(&text)) {
            (401 | 403, _) => Err(Problem::Refused),
            (_, Some(f)) if f.contains("NotAuthorized") || f.contains("FailedAuthentication") => {
                Err(Problem::Refused)
            }
            (_, Some(f)) => Err(Problem::Other(f)),
            (404, None) => Err(Problem::NotFound),
            (200..=299, None) => Ok(text),
            (code, None) => Err(Problem::Other(format!("HTTP {code}"))),
        }
    }
}

/// The fault in an answer, if it is one: its codes and its reason.
fn fault(answer: &str) -> Option<String> {
    let (_, body) = elements(answer, "Fault").into_iter().next()?;
    let codes: Vec<String> = elements(body, "Value")
        .into_iter()
        .map(|(_, v)| unescaped(v.trim()))
        .collect();
    let reason = first_text(body, "Text").unwrap_or_default();
    Some(format!("{} {reason}", codes.join(" ")).trim().to_string())
}

/// The header of WS-Security with the user: the password digested with
/// `nonce` and `created` (SHA-1 of them and of it, in Base64).
fn security(name: &str, password: &str, nonce: &[u8], created: DateTime<Utc>) -> String {
    let created = created.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    let mut sha1 = sha1_smol::Sha1::new();
    sha1.update(nonce);
    sha1.update(created.as_bytes());
    sha1.update(password.as_bytes());
    let digest = BASE64.encode(sha1.digest().bytes());
    format!(
        r#"<Security s:mustUnderstand="1" xmlns="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd"><UsernameToken><Username>{}</Username><Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest">{digest}</Password><Nonce EncodingType="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-soap-message-security-1.0#Base64Binary">{}</Nonce><Created xmlns="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd">{created}</Created></UsernameToken></Security>"#,
        escaped(name),
        BASE64.encode(nonce),
    )
}

/// The elements called `local` (whatever their namespace's prefix): their
/// attributes and what's inside, in order. Enough for the answers of the
/// cameras, which don't nest an element in one of the same name.
fn elements<'a>(xml: &'a str, local: &str) -> Vec<(&'a str, &'a str)> {
    let mut found = vec![];
    let mut at = 0;
    while let Some(open) = xml[at..].find('<').map(|i| at + i) {
        at = open + 1;
        let rest = &xml[at..];
        let name_end = rest
            .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .unwrap_or(rest.len());
        let name = &rest[..name_end];
        if name.rsplit(':').next() != Some(local) || name.starts_with(['/', '?', '!']) {
            continue;
        }
        let Some(tag_end) = rest.find('>') else {
            break;
        };
        let attributes = &rest[name_end..tag_end];
        if let Some(attributes) = attributes.strip_suffix('/') {
            found.push((attributes, ""));
            at += tag_end + 1;
            continue;
        }
        let inside = at + tag_end + 1;
        let close = format!("</{name}>");
        let Some(end) = xml[inside..].find(&close).map(|i| inside + i) else {
            break;
        };
        found.push((attributes, &xml[inside..end]));
        at = end + close.len();
    }
    found
}

/// The text of the first element called `local`.
fn first_text(xml: &str, local: &str) -> Option<String> {
    elements(xml, local)
        .first()
        .map(|(_, body)| unescaped(body.trim()))
}

/// The value of an attribute (whatever its prefix).
fn attribute(attributes: &str, name: &str) -> Option<String> {
    let mut rest = attributes;
    while let Some(eq) = rest.find('=') {
        let key = rest[..eq].trim().rsplit(':').next().unwrap_or_default();
        let after = rest[eq + 1..].trim_start();
        let quote = after.chars().next()?;
        let value_end = after[1..].find(quote)? + 1;
        if key == name {
            return Some(unescaped(&after[1..value_end]));
        }
        rest = &after[value_end + 1..];
    }
    None
}

fn unescaped(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
        },
    };

    #[test]
    fn the_elements_whatever_their_prefix() {
        let xml = r#"<a:Root><b:Profiles token="p0" fixed="true"><tt:Encoding>JPEG</tt:Encoding></b:Profiles><Profiles token='p&amp;1'><tt:Encoding>H264</tt:Encoding></Profiles><x:Empty a="1"/></a:Root>"#;
        assert_eq!(profile_token(xml).as_deref(), Some("p&1"));
        assert_eq!(elements(xml, "Empty"), vec![(r#" a="1""#, "")]);
        assert_eq!(elements(xml, "Encoding").len(), 2);
        assert_eq!(first_text(xml, "Missing"), None);
        let jpeg_only = r#"<Profiles token="only"><Encoding>JPEG</Encoding></Profiles>"#;
        assert_eq!(profile_token(jpeg_only).as_deref(), Some("only"));
    }

    /// The digest of the example of the WS-Security profile used by ONVIF
    /// (its nonce, its time, the password `userpassword`).
    #[test]
    fn the_password_digested() {
        let nonce = BASE64.decode("LKqI6G/AikKCQrN0zqZFlg==").unwrap();
        let created = DateTime::parse_from_rfc3339("2010-09-16T07:50:45Z")
            .unwrap()
            .with_timezone(&Utc);
        let header = security("user", "userpassword", &nonce, created);
        // the Created as the example writes it, with its milliseconds
        let created = first_text(&header, "Created").unwrap();
        let mut sha1 = sha1_smol::Sha1::new();
        sha1.update(&nonce);
        sha1.update(created.as_bytes());
        sha1.update(b"userpassword");
        assert_eq!(
            first_text(&header, "Password").unwrap(),
            BASE64.encode(sha1.digest().bytes())
        );
        // the one of the example, without the milliseconds
        let mut sha1 = sha1_smol::Sha1::new();
        sha1.update(&nonce);
        sha1.update(b"2010-09-16T07:50:45Z");
        sha1.update(b"userpassword");
        assert_eq!(
            BASE64.encode(sha1.digest().bytes()),
            "tuOSpGlFlIXsozq4HFNeeGeFLEI="
        );
        assert_eq!(first_text(&header, "Username").unwrap(), "user");
    }

    const PROBE_MATCH: &str = r#"<?xml version="1.0" encoding="UTF-8"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope" xmlns:d="http://schemas.xmlsoap.org/ws/2005/04/discovery"><SOAP-ENV:Body><d:ProbeMatches><d:ProbeMatch><wsa:EndpointReference><wsa:Address>urn:uuid:1</wsa:Address></wsa:EndpointReference><d:Types>dn:NetworkVideoTransmitter</d:Types><d:Scopes>onvif://www.onvif.org/type/video_encoder onvif://www.onvif.org/name/Campo%20Nord onvif://www.onvif.org/hardware/IPC-B140</d:Scopes><d:XAddrs>http://[fe80::1]/onvif/device_service http://192.168.1.64/onvif/device_service</d:XAddrs></d:ProbeMatch></d:ProbeMatches></SOAP-ENV:Body></SOAP-ENV:Envelope>"#;

    #[test]
    fn a_camera_in_an_answer() {
        assert_eq!(
            matches(PROBE_MATCH),
            vec![Found {
                name: "Campo Nord".into(),
                address: "http://192.168.1.64/onvif/device_service".into(),
            }]
        );
        let unnamed = PROBE_MATCH.replace("onvif://www.onvif.org/name/Campo%20Nord ", "");
        assert_eq!(matches(&unnamed)[0].name, "IPC-B140");
        let bare = unnamed.replace("onvif://www.onvif.org/hardware/IPC-B140", "");
        assert_eq!(matches(&bare)[0].name, "192.168.1.64");
        assert_eq!(matches(&bare)[0].host(), "192.168.1.64");
        assert!(matches("<a>nothing</a>").is_empty());
    }

    /// The probe sent, the answer read: a camera answering on this
    /// computer, as one on the network would (to the probe's socket).
    #[test]
    fn found_by_its_answer() {
        let camera = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let camera_at = camera.local_addr().unwrap();
        let ours = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        ours.set_read_timeout(Some(Duration::from_millis(50)))
            .unwrap();
        let answering = std::thread::spawn(move || {
            let mut buffer = vec![0u8; 65_535];
            let (read, from) = camera.recv_from(&mut buffer).unwrap();
            let probe = String::from_utf8_lossy(&buffer[..read]).to_string();
            assert!(probe.contains("NetworkVideoTransmitter"), "{probe}");
            // twice, as cameras do: found once
            camera.send_to(PROBE_MATCH.as_bytes(), from).unwrap();
            camera.send_to(PROBE_MATCH.as_bytes(), from).unwrap();
        });
        ours.send_to(probe_message("1").as_bytes(), camera_at)
            .unwrap();
        let found = collect(std::slice::from_ref(&ours), Duration::from_millis(500));
        answering.join().unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Campo Nord");
    }

    /// A camera answering ONVIF over HTTP: the clock, the media service
    /// elsewhere, two profiles (the H.264 one second), its RTSP address at
    /// `0.0.0.0`; the password checked by its digest.
    async fn camera(password: &'static str) -> u16 {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((mut tcp, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut request = vec![];
                    let mut buffer = [0u8; 8192];
                    loop {
                        let Ok(read) = tcp.read(&mut buffer).await else {
                            return;
                        };
                        request.extend_from_slice(&buffer[..read]);
                        let text = String::from_utf8_lossy(&request);
                        if let Some(head_end) = text.find("\r\n\r\n") {
                            let length = text[..head_end]
                                .lines()
                                .find_map(|l| {
                                    l.to_lowercase()
                                        .strip_prefix("content-length:")
                                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                                })
                                .unwrap_or(0);
                            if request.len() >= head_end + 4 + length || read == 0 {
                                break;
                            }
                        }
                    }
                    let text = String::from_utf8_lossy(&request).to_string();
                    let (status, body) = answer(&text, password);
                    let response = format!(
                        "HTTP/1.1 {status}\r\nContent-Type: application/soap+xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = tcp.write_all(response.as_bytes()).await;
                    let _ = tcp.shutdown().await;
                });
            }
        });
        port
    }

    fn envelope(body: &str) -> String {
        format!(
            r#"<?xml version="1.0"?><env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope" xmlns:tt="http://www.onvif.org/ver10/schema" xmlns:trt="http://www.onvif.org/ver10/media/wsdl"><env:Body>{body}</env:Body></env:Envelope>"#
        )
    }

    /// The camera's answer; its clock two hours ahead.
    fn answer(request: &str, password: &str) -> (&'static str, String) {
        let now = Utc::now() + TimeDelta::hours(2);
        if request.contains("GetSystemDateAndTime") {
            use chrono::{Datelike, Timelike};
            return (
                "200 OK",
                envelope(&format!(
                    "<tds:GetSystemDateAndTimeResponse><tds:SystemDateAndTime><tt:UTCDateTime><tt:Time><tt:Hour>{}</tt:Hour><tt:Minute>{}</tt:Minute><tt:Second>{}</tt:Second></tt:Time><tt:Date><tt:Year>{}</tt:Year><tt:Month>{}</tt:Month><tt:Day>{}</tt:Day></tt:Date></tt:UTCDateTime></tds:SystemDateAndTime></tds:GetSystemDateAndTimeResponse>",
                    now.hour(),
                    now.minute(),
                    now.second(),
                    now.year(),
                    now.month(),
                    now.day()
                )),
            );
        }
        let authorized = (|| {
            let nonce = BASE64.decode(first_text(request, "Nonce")?).ok()?;
            let created = first_text(request, "Created")?;
            let at = DateTime::parse_from_rfc3339(&created).ok()?;
            if (at.with_timezone(&Utc) - now).num_seconds().abs() > 5 {
                return None;
            }
            let mut sha1 = sha1_smol::Sha1::new();
            sha1.update(&nonce);
            sha1.update(created.as_bytes());
            sha1.update(password.as_bytes());
            (first_text(request, "Password")? == BASE64.encode(sha1.digest().bytes())).then_some(())
        })()
        .is_some();
        if !authorized {
            return (
                "400 Bad Request",
                envelope(
                    "<env:Fault><env:Code><env:Value>env:Sender</env:Value><env:Subcode><env:Value>ter:NotAuthorized</env:Value></env:Subcode></env:Code><env:Reason><env:Text xml:lang=\"en\">Sender not Authorized</env:Text></env:Reason></env:Fault>",
                ),
            );
        }
        let host = request
            .lines()
            .find_map(|l| {
                l.strip_prefix("host: ")
                    .or_else(|| l.strip_prefix("Host: "))
            })
            .unwrap_or_default()
            .trim()
            .to_string();
        match () {
            () if request.contains("GetCapabilities") => (
                "200 OK",
                envelope(&format!(
                    "<tds:GetCapabilitiesResponse><tds:Capabilities><tt:Device><tt:XAddr>http://{host}/onvif/device_service</tt:XAddr></tt:Device><tt:Media><tt:XAddr>http://0.0.0.0:{}/onvif/media</tt:XAddr></tt:Media></tds:Capabilities></tds:GetCapabilitiesResponse>",
                    host.rsplit(':').next().unwrap_or_default()
                )),
            ),
            () if request.contains("GetProfiles") && request.starts_with("POST /onvif/media") => (
                "200 OK",
                envelope(
                    r#"<trt:GetProfilesResponse><trt:Profiles fixed="true" token="Mjpeg"><tt:Name>mjpeg</tt:Name><tt:VideoEncoderConfiguration token="v0"><tt:Encoding>JPEG</tt:Encoding></tt:VideoEncoderConfiguration></trt:Profiles><trt:Profiles fixed="true" token="Main&amp;HD"><tt:Name>main</tt:Name><tt:VideoEncoderConfiguration token="v1"><tt:Encoding>H264</tt:Encoding></tt:VideoEncoderConfiguration></trt:Profiles></trt:GetProfilesResponse>"#,
                ),
            ),
            () if request.contains("GetStreamUri")
                && request.contains("<ProfileToken>Main&amp;HD</ProfileToken>") =>
            {
                (
                    "200 OK",
                    envelope(
                        "<trt:GetStreamUriResponse><trt:MediaUri><tt:Uri>rtsp://0.0.0.0:554/Streaming/Channels/101?transportmode=unicast&amp;profile=Profile_1</tt:Uri><tt:InvalidAfterConnect>false</tt:InvalidAfterConnect></trt:MediaUri></trt:GetStreamUriResponse>",
                    ),
                )
            }
            () => ("404 Not Found", String::new()),
        }
    }

    #[test]
    fn the_rtsp_address_asked_of_the_camera() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let port = camera("segreta").await;
            let device =
                url::Url::parse(&format!("http://127.0.0.1:{port}/onvif/device_service")).unwrap();
            let user = |password: &str| Some(("admin".to_string(), password.to_string()));
            let uri = stream_uri(&device, user("segreta")).await.unwrap();
            assert_eq!(
                uri.as_str(),
                "rtsp://127.0.0.1:554/Streaming/Channels/101?transportmode=unicast&profile=Profile_1"
            );
            assert_eq!(
                stream_uri(&device, user("sbagliata")).await,
                Err(Problem::Refused)
            );
            assert_eq!(stream_uri(&device, None).await, Err(Problem::Refused));
            let nobody = url::Url::parse("http://127.0.0.1:9/onvif/device_service").unwrap();
            assert_eq!(
                stream_uri(&nobody, user("segreta")).await,
                Err(Problem::Unreachable)
            );
        });
    }

    /// A camera on the network of this computer for real (a pretend one
    /// answering WS-Discovery and the SOAP calls, its RTSP address that of
    /// the test server): found, and its pictures through its ONVIF
    /// address. `cargo test -p scout4all-desktop -- --ignored onvif_on`.
    #[test]
    #[ignore]
    fn onvif_on_the_network() {
        use crate::live::{
            capture::{Capture, Source},
            network::NetworkSource,
        };
        let found = discover(SEARCH_WITHIN);
        let camera = found.first().expect("no camera answered").clone();
        let capture = Capture::open(Source::Network(NetworkSource {
            url: camera.address.clone(),
            username: "scout".into(),
            password: "segreta".into(),
        }));
        let seen = (0..150).any(|_| {
            std::thread::sleep(Duration::from_millis(100));
            capture.status().is_some_and(|s| s.frame.is_some())
        });
        assert!(seen, "{camera:?} {:?}", capture.status());
    }

    #[test]
    fn the_onvif_addresses() {
        let onvif = |a: &str| is_onvif(&url::Url::parse(a).unwrap());
        assert!(onvif("http://192.168.1.64/onvif/device_service"));
        assert!(onvif("https://cam:8443/ONVIF/device_service"));
        assert!(!onvif("https://cdn.example/live/onvif.m3u8"));
        assert!(!onvif("rtsp://192.168.1.64/onvif/stream"));
    }
}
