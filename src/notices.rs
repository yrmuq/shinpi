use serde::{Deserialize, Serialize};
use std::{
    sync::atomic::{AtomicI32, Ordering},
    time::{Duration, Instant},
};

pub const LIMIT: usize = 20;
const DISMISS_DELAY: Duration = Duration::from_millis(160);

#[derive(Clone, Deserialize, Serialize)]
pub struct Note {
    #[serde(skip, default = "next_id")]
    pub id: i32,
    pub message: String,
    pub at: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub test: bool,
    #[serde(skip)]
    dismissed_at: Option<Instant>,
}

impl Note {
    pub fn closing(&self) -> bool {
        self.dismissed_at.is_some()
    }
}

fn next_id() -> i32 {
    static NEXT: AtomicI32 = AtomicI32::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

pub fn push(notes: &mut Vec<Note>, message: String, at: u64, test: bool) {
    notes.push(Note {
        id: next_id(),
        message,
        at,
        test,
        dismissed_at: None,
    });
    notes.drain(..notes.len().saturating_sub(LIMIT));
}

pub fn dismiss(notes: &mut [Note], id: i32, now: Instant) -> bool {
    let Some(note) = notes
        .iter_mut()
        .find(|note| note.id == id && !note.closing())
    else {
        return false;
    };
    note.dismissed_at = Some(now);
    true
}

pub fn remove_dismissed(notes: &mut Vec<Note>, now: Instant) -> bool {
    let count = notes.len();
    notes.retain(|note| {
        note.dismissed_at
            .is_none_or(|at| now.duration_since(at) < DISMISS_DELAY)
    });
    count != notes.len()
}

pub fn ago(at: u64, now: u64, catalog: &crate::i18n::Catalog) -> String {
    let secs = now.saturating_sub(at);
    if secs < 60 {
        return catalog.tr("Just now");
    }
    let (n, forms) = if secs < 3600 {
        (
            secs / 60,
            [
                "{} minute ago",
                "{} minutes ago [few]",
                "{} minutes ago [many]",
            ],
        )
    } else if secs < 86400 {
        (
            secs / 3600,
            ["{} hour ago", "{} hours ago [few]", "{} hours ago [many]"],
        )
    } else {
        (
            secs / 86400,
            ["{} day ago", "{} days ago [few]", "{} days ago [many]"],
        )
    };
    // shortcut: English/Russian plural rules; extend when bundling another language.
    let form = if catalog.language == "Russian" {
        if (11..=14).contains(&(n % 100)) {
            2
        } else {
            match n % 10 {
                1 => 0,
                2..=4 => 1,
                _ => 2,
            }
        }
    } else if n == 1 {
        0
    } else {
        2
    };
    catalog.trf(forms[form], &[&n.to_string()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_keeps_latest_records_restores_and_dismisses_only_chosen_note() {
        let mut notes = Vec::new();
        for at in 1..=22 {
            push(&mut notes, at.to_string(), at, at == 5);
        }
        assert_eq!(notes.len(), LIMIT);
        assert_eq!(notes[0].at, 3);
        let raw = serde_json::to_string(&notes).unwrap();
        let mut restored: Vec<Note> = serde_json::from_str(&raw).unwrap();
        assert!(!restored[0].closing());
        assert!(!restored[0].test);
        let id = restored[1].id;
        let even_after_shift = restored[2].id;
        let now = Instant::now();
        assert!(dismiss(&mut restored, id, now));
        assert!(!dismiss(&mut restored, id, now));
        assert!(!remove_dismissed(&mut restored, now));
        assert!(remove_dismissed(&mut restored, now + DISMISS_DELAY));
        assert_eq!(restored.len(), LIMIT - 1);
        assert_eq!(restored[0].at, 3);
        assert_eq!(restored[1].at, 5);
        assert!(restored[1].test);
        assert!(dismiss(&mut restored, even_after_shift, now));
        assert_eq!(restored.iter().find(|note| note.closing()).unwrap().at, 5);
        assert_eq!(
            ago(100, 99, &crate::i18n::Catalog::load("Russian")),
            "Только что"
        );
        assert_eq!(
            ago(100, 100 + 21 * 60, &crate::i18n::Catalog::load("Russian")),
            "21 минуту назад"
        );
        assert_eq!(
            ago(100, 100 + 12 * 3600, &crate::i18n::Catalog::load("Russian")),
            "12 часов назад"
        );
    }
}
