//! Per-profile, per-item playback-version memory.
use super::{MemoryEntry, Parse, ProfileList, Row, Scope, SettingsWrite};
use crate::plex::session::Session;
use serde::{Deserialize, Serialize};

/// The most recently selected version for one item, on one server, for one profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayVersionEntry {
    pub machine_id: String,
    pub rating_key: String,
    pub media_id: i64,
}

impl MemoryEntry for PlayVersionEntry {
    type Key = (String, String);
    const CAP: usize = 32;
    const ACCOUNT_BOUND: bool = true;

    fn key(&self) -> Self::Key {
        (self.machine_id.clone(), self.rating_key.clone())
    }

    fn matches(&self, key: &Self::Key) -> bool {
        self.machine_id == key.0 && self.rating_key == key.1
    }

    fn forgets(&self) -> bool {
        self.machine_id.is_empty() || self.rating_key.is_empty() || self.media_id <= 0
    }

    fn normalized(mut self) -> Self {
        if self.machine_id.len() > 64 { self.machine_id.clear(); }
        if self.rating_key.len() > 16 { self.rating_key.clear(); }
        self
    }

    fn slot(session: &Session) -> &Vec<ProfileList<Self>> {
        &session.prefs.play_versions
    }

    fn slot_mut(session: &mut Session) -> &mut Vec<ProfileList<Self>> {
        &mut session.prefs.play_versions
    }
}

/// Descriptor for the memory-door list. It is intentionally not a `ListRow`: all writes must
/// merge through `ProfileMemory::upsert_with` so one pick cannot overwrite another profile/item.
pub struct PlayVersionPref;

impl Row for PlayVersionPref {
    const KEY: &'static str = "play_versions";
    const SCOPE: Scope = Scope::PerItem;
    const ACCOUNT_BOUND: bool = true;
    const LIVE: bool = false;
    const SETTINGS: Option<SettingsWrite> = None;
    const PARSE: Parse = Parse::SoftVec;
}
