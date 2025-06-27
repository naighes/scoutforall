use chrono::{DateTime, FixedOffset, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use crate::shapes::set::SetEntry;

#[derive(Debug)]
pub enum MenuFlow {
    Continue,
    Back,
}

pub enum ContinueMatchResult {
    SetToPlay(SetEntry),
    MatchFinished,
}
