//! An example training plan for the screenshots (`scripts/generate_screenshots.py`,
//! `scripts/generate_phone_screenshots.py`): written through the core into
//! the database of a data folder already migrated, for its team, around
//! the day given (the week the calendar opens on).
//!
//!     cargo run -p scout4all-core --example training_plan -- <data> <team id> <lang> [YYYY-MM-DD]
//!
//! A gym; a program of 16 weeks holding 4 mesocycles of 4 weeks; a
//! microcycle a week from 3 weeks before to 2 after; three sessions a week
//! in each (Monday, Wednesday, Friday), the past ones completed with their
//! activities and attendance, one cancelled, the others planned. A day
//! with a match of the team has no session.

use chrono::{Duration, Local, NaiveDate, NaiveTime};
use scout4all_core::{
    db,
    errors::AppError,
    providers::{
        sqlite::{
            Database, teams::SqliteTeamReader, training::SqliteTraining, venues::SqliteVenues,
        },
        team_reader::TeamReader,
        training_cycles::TrainingCycles,
        training_writer::TrainingWriter,
    },
    training::{
        ActivityInput, ActivityStatus, AttendanceInput, AttendanceStatus, SessionInput,
        SessionStatus,
        cycles::{CycleInput, Level, Span, monday},
        venue::VenueInput,
    },
};
use std::path::PathBuf;
use uuid::Uuid;

/// The words of the plan in a language.
struct Words {
    gym: &'static str,
    city: &'static str,
    program: &'static str,
    program_goals: &'static str,
    /// the title and the objectives of each mesocycle
    mesocycles: [(&'static str, &'static str); 4],
    /// the title (its theme) of each microcycle, from the first
    microcycles: [&'static str; 6],
    /// the objectives of every microcycle
    week_goals: &'static str,
    /// the title of the session on Monday, Wednesday, Friday
    sessions: [&'static str; 3],
    /// name, description, minutes
    activities: [(&'static str, &'static str, u16); 6],
    remarks: &'static str,
    cancelled: &'static str,
    late: &'static str,
    injured: &'static str,
}

const EN: Words = Words {
    gym: "Centro Pavesi",
    city: "Milan",
    program: "Season 2026",
    program_goals: "Peak form for the finals in January.",
    mesocycles: [
        ("General preparation", "Strength and volume."),
        ("Specific preparation", "Technique under pressure."),
        ("Competition", "Game situations, quality of the side-out."),
        ("Finals", "Freshness and set plays."),
    ],
    microcycles: [
        "Serve and reception",
        "Attack against the block",
        "Defense and transition",
        "Side-out under pressure",
        "Break point",
        "Set plays",
    ],
    week_goals: "Quality of the first touch, intensity of the drills.",
    sessions: ["Technique", "Tactics", "Game"],
    activities: [
        ("Warm-up", "Mobility and jumps", 15),
        (
            "Serve and reception",
            "Float and jump serves, reception in 3",
            30,
        ),
        (
            "Attack against the block",
            "Outside and opposite against 2 blockers",
            30,
        ),
        ("Defense and transition", "Dig, set, counter-attack", 25),
        ("Block", "Reading the setter, closing the line", 20),
        ("Game 6 v 6", "Sets to 15 from 10-10", 20),
    ],
    remarks: "Good intensity, reception to work on.",
    cancelled: "Gym closed",
    late: "Traffic",
    injured: "Ankle",
};

const IT: Words = Words {
    gym: "Centro Pavesi",
    city: "Milano",
    program: "Stagione 2026",
    program_goals: "Forma migliore per le finali di gennaio.",
    mesocycles: [
        ("Preparazione generale", "Forza e volume."),
        ("Preparazione specifica", "Tecnica sotto pressione."),
        (
            "Agonistico",
            "Situazioni di gioco, qualità del cambio palla.",
        ),
        ("Finali", "Freschezza e schemi."),
    ],
    microcycles: [
        "Battuta e ricezione",
        "Attacco contro muro",
        "Difesa e contrattacco",
        "Cambio palla sotto pressione",
        "Break point",
        "Schemi",
    ],
    week_goals: "Qualità del primo tocco, intensità degli esercizi.",
    sessions: ["Tecnica", "Tattica", "Gioco"],
    activities: [
        ("Riscaldamento", "Mobilità e salti", 15),
        (
            "Battuta e ricezione",
            "Flottante e salto, ricezione a 3",
            30,
        ),
        ("Attacco contro muro", "Banda e opposto contro 2 a muro", 30),
        ("Difesa e contrattacco", "Difesa, alzata, contrattacco", 25),
        ("Muro", "Lettura dell'alzatore, chiusura del lungolinea", 20),
        ("Gioco 6 contro 6", "Set a 15 dal 10-10", 20),
    ],
    remarks: "Buona intensità, ricezione da curare.",
    cancelled: "Palestra chiusa",
    late: "Traffico",
    injured: "Caviglia",
};

const PT: Words = Words {
    gym: "Centro Pavesi",
    city: "Milão",
    program: "Temporada 2026",
    program_goals: "Melhor forma para as finais de janeiro.",
    mesocycles: [
        ("Preparação geral", "Força e volume."),
        ("Preparação específica", "Técnica sob pressão."),
        ("Competição", "Situações de jogo, qualidade do side-out."),
        ("Finais", "Frescor e jogadas ensaiadas."),
    ],
    microcycles: [
        "Saque e recepção",
        "Ataque contra o bloqueio",
        "Defesa e contra-ataque",
        "Side-out sob pressão",
        "Break point",
        "Jogadas ensaiadas",
    ],
    week_goals: "Qualidade do primeiro toque, intensidade dos exercícios.",
    sessions: ["Técnica", "Tática", "Jogo"],
    activities: [
        ("Aquecimento", "Mobilidade e saltos", 15),
        (
            "Saque e recepção",
            "Saque flutuante e viagem, recepção em 3",
            30,
        ),
        (
            "Ataque contra o bloqueio",
            "Ponteira e oposta contra 2 bloqueadoras",
            30,
        ),
        (
            "Defesa e contra-ataque",
            "Defesa, levantamento, contra-ataque",
            25,
        ),
        ("Bloqueio", "Leitura da levantadora, fechar a paralela", 20),
        ("Jogo 6 x 6", "Sets de 15 a partir de 10-10", 20),
    ],
    remarks: "Boa intensidade, recepção a melhorar.",
    cancelled: "Ginásio fechado",
    late: "Trânsito",
    injured: "Tornozelo",
};

const PL: Words = Words {
    gym: "Centro Pavesi",
    city: "Mediolan",
    program: "Sezon 2026",
    program_goals: "Najlepsza forma na finały w styczniu.",
    mesocycles: [
        ("Przygotowanie ogólne", "Siła i objętość."),
        ("Przygotowanie specjalne", "Technika pod presją."),
        ("Okres startowy", "Sytuacje meczowe, jakość side-outu."),
        ("Finały", "Świeżość i zagrywki taktyczne."),
    ],
    microcycles: [
        "Zagrywka i przyjęcie",
        "Atak przeciw blokowi",
        "Obrona i kontratak",
        "Side-out pod presją",
        "Break point",
        "Zagrywki taktyczne",
    ],
    week_goals: "Jakość pierwszego kontaktu, intensywność ćwiczeń.",
    sessions: ["Technika", "Taktyka", "Gra"],
    activities: [
        ("Rozgrzewka", "Mobilność i skoki", 15),
        (
            "Zagrywka i przyjęcie",
            "Zagrywka szybująca i z wyskoku, przyjęcie w 3",
            30,
        ),
        (
            "Atak przeciw blokowi",
            "Przyjmująca i atakująca przeciw 2 blokującym",
            30,
        ),
        ("Obrona i kontratak", "Obrona, wystawa, kontratak", 25),
        ("Blok", "Czytanie rozgrywającej, zamykanie linii", 20),
        ("Gra 6 na 6", "Sety do 15 od 10-10", 20),
    ],
    remarks: "Dobra intensywność, przyjęcie do poprawy.",
    cancelled: "Hala zamknięta",
    late: "Korki",
    injured: "Kostka",
};

const JA: Words = Words {
    gym: "Centro Pavesi",
    city: "ミラノ",
    program: "2026シーズン",
    program_goals: "1月の決勝に向けて最高のコンディションを。",
    mesocycles: [
        ("一般的準備期", "筋力と練習量。"),
        ("専門的準備期", "プレッシャー下の技術。"),
        ("試合期", "ゲーム形式、サイドアウトの質。"),
        ("決勝", "コンディション維持とセットプレー。"),
    ],
    microcycles: [
        "サーブとレシーブ",
        "ブロックに対するアタック",
        "ディフェンスと切り返し",
        "プレッシャー下のサイドアウト",
        "ブレイク",
        "セットプレー",
    ],
    week_goals: "ファーストタッチの質、練習の強度。",
    sessions: ["技術", "戦術", "ゲーム"],
    activities: [
        ("ウォームアップ", "可動域とジャンプ", 15),
        (
            "サーブとレシーブ",
            "フローターとジャンプサーブ、3人レシーブ",
            30,
        ),
        (
            "ブロックに対するアタック",
            "アウトサイドとオポジット対2枚ブロック",
            30,
        ),
        ("ディフェンスと切り返し", "ディグ、トス、カウンター", 25),
        ("ブロック", "セッターを読む、ストレートを閉める", 20),
        ("6対6ゲーム", "10-10から15点セット", 20),
    ],
    remarks: "強度は良好、レシーブに課題。",
    cancelled: "体育館休館",
    late: "渋滞",
    injured: "足首",
};

fn words(lang: &str) -> Option<&'static Words> {
    match lang {
        "en" => Some(&EN),
        "it" => Some(&IT),
        "pt" => Some(&PT),
        "pl" => Some(&PL),
        "ja" => Some(&JA),
        _ => None,
    }
}

/// The activities of the session on the day of the week (0 Monday, 1
/// Wednesday, 2 Friday): the warm-up, two of the week, the game.
fn activities_of(w: &Words, day: usize, week: usize) -> Vec<(&'static str, &'static str, u16)> {
    let middle = [1 + (week + day) % 4, 1 + (week + day + 1) % 4];
    std::iter::once(w.activities[0])
        .chain(middle.iter().filter_map(|i| w.activities.get(*i).copied()))
        .chain(std::iter::once(w.activities[5]))
        .collect()
}

/// The days of the matches of the team (as either side), not removed.
fn match_days(db: &rusqlite::Connection, team: &Uuid) -> rusqlite::Result<Vec<NaiveDate>> {
    db.prepare(
        "SELECT DISTINCT substr(start, 1, 10) FROM matches
         WHERE deleted = 0 AND (team_a = ?1 OR team_b = ?1)",
    )?
    .query_map([team.to_string()], |row| row.get::<_, String>(0))?
    .map(|day| day.map(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").unwrap_or_default()))
    .collect()
}

fn usage() -> String {
    "usage: training_plan <data folder> <team id> <en|it|pt|pl|ja> [YYYY-MM-DD]".into()
}

fn fail(e: AppError) -> String {
    format!("{e:?}")
}

// a program run by hand, the edge as the clock of the apps: it reads the
// time of the system (see clippy.toml)
#[allow(clippy::disallowed_methods)]
async fn plan(
    data: PathBuf,
    team_id: Uuid,
    w: &'static Words,
    today: NaiveDate,
) -> Result<(), String> {
    let connection = db::open(&data.join("scout4all.db")).map_err(fail)?;
    let match_days = match_days(&connection, &team_id).map_err(|e| e.to_string())?;
    let database = Database::new(connection);
    let team = SqliteTeamReader::new(database.clone())
        .read_single(&team_id)
        .await
        .map_err(fail)?;
    let players: Vec<_> = team.players.iter().filter(|p| !p.deleted).collect();
    let training = SqliteTraining::new(database.clone());
    let gym = SqliteVenues::new(database.clone())
        .create(VenueInput {
            name: w.gym.into(),
            address: w.city.into(),
            map_url: String::new(),
            coordinates: None,
            notes: String::new(),
            pinned: true,
        })
        .map_err(fail)?;
    let this_week = monday(today);
    let weeks = |n: i64| this_week + Duration::weeks(n);
    let cycle = |parent_id, title: &str, start, span, objectives: &str| CycleInput {
        parent_id,
        title: title.into(),
        start,
        span,
        objectives: objectives.into(),
        notes: String::new(),
    };
    let program = training
        .create_cycle(
            &team_id,
            Level::Program,
            cycle(
                None,
                w.program,
                weeks(-8),
                Span::Until(weeks(8) - Duration::days(1)),
                w.program_goals,
            ),
            vec![],
        )
        .await
        .map_err(fail)?;
    let mut mesocycles = Vec::new();
    for (i, (title, goals)) in w.mesocycles.iter().enumerate() {
        let start = weeks(-8 + 4 * i as i64);
        mesocycles.push((
            start,
            training
                .create_cycle(
                    &team_id,
                    Level::Mesocycle,
                    cycle(Some(program.id), title, start, Span::Weeks(4), goals),
                    vec![],
                )
                .await
                .map_err(fail)?,
        ));
    }
    let now = Local::now().naive_local();
    for (week, goals) in w.microcycles.iter().enumerate() {
        let start = weeks(week as i64 - 3);
        let parent = mesocycles
            .iter()
            .rev()
            .find(|(s, _)| *s <= start)
            .map(|(_, m)| m.id);
        let micro = training
            .create_cycle(
                &team_id,
                Level::Microcycle,
                cycle(parent, goals, start, Span::Weeks(1), w.week_goals),
                vec![],
            )
            .await
            .map_err(fail)?;
        for (day, title) in w.sessions.iter().enumerate() {
            let date = start + Duration::days(2 * day as i64);
            if match_days.contains(&date) {
                continue;
            }
            let at = date.and_time(NaiveTime::from_hms_opt(18, 0, 0).unwrap_or_default());
            let minutes: u16 = if day == 2 { 90 } else { 120 };
            let past = at + Duration::minutes(minutes.into()) < now;
            let cancelled = past && week == 1 && day == 1;
            let status = match (past, cancelled) {
                (_, true) => SessionStatus::Cancelled,
                (true, false) => SessionStatus::Completed,
                (false, _) => SessionStatus::Planned,
            };
            let session = training
                .create_session(
                    &team_id,
                    SessionInput {
                        microcycle_id: Some(micro.id),
                        start: at,
                        duration_min: minutes,
                        actual_duration_min: (status == SessionStatus::Completed)
                            .then_some(minutes),
                        venue_id: Some(gym.id),
                        title: (*title).into(),
                        objectives: (*goals).into(),
                        notes: if cancelled {
                            w.cancelled.into()
                        } else {
                            String::new()
                        },
                        final_remarks: if status == SessionStatus::Completed && day == 0 {
                            w.remarks.into()
                        } else {
                            String::new()
                        },
                        status,
                    },
                )
                .await
                .map_err(fail)?;
            for (name, description, planned) in activities_of(w, day, week) {
                training
                    .add_activity(
                        &session.id,
                        ActivityInput {
                            name: name.into(),
                            description: description.into(),
                            planned_min: Some(planned),
                            actual_min: (status == SessionStatus::Completed).then_some(planned),
                            notes: String::new(),
                            status: match status {
                                SessionStatus::Completed => ActivityStatus::Completed,
                                _ => ActivityStatus::Planned,
                            },
                        },
                    )
                    .await
                    .map_err(fail)?;
            }
            if status == SessionStatus::Completed {
                let rows = players
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let (status, note) = match (i + week + day) % 11 {
                            3 if i % 2 == 0 => (AttendanceStatus::Late, w.late),
                            7 if week < 2 => (AttendanceStatus::Unavailable, w.injured),
                            9 => (AttendanceStatus::Absent, ""),
                            _ => (AttendanceStatus::Present, ""),
                        };
                        AttendanceInput {
                            person_id: p.id,
                            number: Some(p.number),
                            status,
                            note: note.into(),
                            guest: false,
                            makeup: false,
                        }
                    })
                    .collect();
                training
                    .save_attendance(&session.id, rows)
                    .await
                    .map_err(fail)?;
            }
        }
    }
    Ok(())
}

// a program run by hand, the edge as the clock of the apps: it reads the
// time of the system (see clippy.toml)
#[allow(clippy::disallowed_methods)]
#[tokio::main]
async fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (data, team, lang, day) = match args.as_slice() {
        [data, team, lang] => (data, team, lang, None),
        [data, team, lang, day] => (data, team, lang, Some(day)),
        _ => return Err(usage()),
    };
    let team_id = Uuid::parse_str(team).map_err(|e| format!("{team}: {e}"))?;
    let w = words(lang).ok_or_else(usage)?;
    let today = match day {
        Some(d) => NaiveDate::parse_from_str(d, "%Y-%m-%d").map_err(|e| format!("{d}: {e}"))?,
        None => Local::now().date_naive(),
    };
    plan(PathBuf::from(data), team_id, w, today).await
}
