use std::{path::PathBuf, sync::Arc};

use crate::{
    providers::{
        match_reader::MatchReader, match_writer::MatchWriter, set_writer::SetWriter,
        settings_reader::SettingsReader, settings_writer::SettingsWriter, team_reader::TeamReader,
        team_writer::TeamWriter,
    },
    screens::{screen::ScreenAsync, team_list_screen::TeamListScreen},
    shapes::{settings::Settings, team::TeamEntry},
};

pub struct App {
    screens: Vec<Box<dyn ScreenAsync>>,
}

impl App {
    pub fn new<
        TR: TeamReader + Send + Sync + 'static,
        TW: TeamWriter + Send + Sync + 'static,
        SW: SettingsWriter + Send + Sync + 'static,
        MR: MatchReader + Send + Sync + 'static,
        MW: MatchWriter + Send + Sync + 'static,
        SSW: SetWriter + Send + Sync + 'static,
        SR: SettingsReader + Send + Sync + 'static,
    >(
        settings: Settings,
        teams: Vec<TeamEntry>,
        base_path: PathBuf,
        team_reader: Arc<TR>,
        team_writer: Arc<TW>,
        settings_writer: Arc<SW>,
        set_writer: Arc<SSW>,
        match_reader: Arc<MR>,
        match_writer: Arc<MW>,
        settings_reader: Arc<SR>,
    ) -> Self {
        Self {
            screens: vec![Box::new(TeamListScreen::new(
                settings,
                teams,
                base_path,
                team_reader,
                team_writer,
                settings_writer,
                match_reader,
                match_writer,
                set_writer,
                settings_reader,
            ))],
        }
    }

    pub fn current_screen(&mut self) -> Option<&mut Box<dyn ScreenAsync>> {
        self.screens.last_mut()
    }

    pub fn push_screen(&mut self, screen: Box<dyn ScreenAsync>) {
        self.screens.push(screen);
    }

    pub async fn pop_screen(&mut self, refresh: bool, count: Option<u8>) {
        let count = count.unwrap_or(0) as usize;
        if count > 0 {
            let to_pop = count.min(self.screens.len().saturating_sub(1));
            for _ in 0..to_pop {
                self.screens.pop();
            }
            if let Some(prev) = self.screens.last_mut() {
                if refresh {
                    prev.refresh_data().await;
                }
            }
        }
    }
}
