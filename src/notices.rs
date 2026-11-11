use serde::{Deserialize, Serialize};

pub const LIMIT: usize = 20;

#[derive(Clone, Deserialize, Serialize)]
pub struct Note {
    pub message: String,
    pub at: u64,
    #[serde(skip, default = "visible")]
    pub fade: f32,
    #[serde(skip)]
    pub closing: bool,
}

fn visible() -> f32 {
    1.0
}

pub fn push(notes: &mut Vec<Note>, message: String, at: u64, animate: bool) {
    notes.push(Note {
        message,
        at,
        fade: if animate { 0.0 } else { 1.0 },
        closing: false,
    });
    notes.drain(..notes.len().saturating_sub(LIMIT));
}

pub fn step(notes: &mut Vec<Note>) -> bool {
    let mut changed = false;
    for note in notes.iter_mut() {
        let target = if note.closing { 0.0 } else { 1.0 };
        if note.fade != target {
            note.fade = if note.closing {
                (note.fade - 1.0 / 3.0).max(0.0)
            } else {
                (note.fade + 1.0 / 3.0).min(1.0)
            };
            changed = true;
        }
    }
    let count = notes.len();
    notes.retain(|note| !note.closing || note.fade > 0.0);
    changed || count != notes.len()
}

pub fn ago(at: u64, now: u64) -> String {
    let secs = now.saturating_sub(at);
    if secs < 60 {
        return "Только что".into();
    }
    let (n, forms) = if secs < 3600 {
        (secs / 60, ["минуту", "минуты", "минут"])
    } else if secs < 86400 {
        (secs / 3600, ["час", "часа", "часов"])
    } else {
        (secs / 86400, ["день", "дня", "дней"])
    };
    let form = if (11..=14).contains(&(n % 100)) {
        2
    } else {
        match n % 10 {
            1 => 0,
            2..=4 => 1,
            _ => 2,
        }
    };
    format!("{n} {} назад", forms[form])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_keeps_latest_records_restores_and_dismisses_only_chosen_note() {
        let mut notes = Vec::new();
        for at in 1..=22 {
            push(&mut notes, at.to_string(), at, true);
        }
        assert_eq!(notes.len(), LIMIT);
        assert_eq!(notes[0].at, 3);
        let raw = serde_json::to_string(&notes).unwrap();
        let mut restored: Vec<Note> = serde_json::from_str(&raw).unwrap();
        assert_eq!(restored[0].fade, 1.0);
        restored[1].closing = true;
        for _ in 0..4 {
            step(&mut restored);
        }
        assert_eq!(restored.len(), LIMIT - 1);
        assert_eq!(restored[0].at, 3);
        assert_eq!(restored[1].at, 5);
        assert_eq!(ago(100, 99), "Только что");
        assert_eq!(ago(100, 100 + 21 * 60), "21 минуту назад");
        assert_eq!(ago(100, 100 + 12 * 3600), "12 часов назад");
    }
}
