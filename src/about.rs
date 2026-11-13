use std::time::{Duration, Instant};

pub struct Part {
    pub name: String,
    pub upstream: String,
    pub detail: String,
    pub url: String,
}

pub fn parts() -> Vec<Part> {
    include_str!("../README.md")
        .lines()
        .skip_while(|line| *line != "## License & Attribution")
        .filter_map(|line| {
            let cells: Vec<_> = line.trim_matches('|').split('|').map(str::trim).collect();
            if cells.len() != 4 {
                return None;
            }
            let (upstream, url) = cells[1].strip_prefix('[')?.split_once("](")?;
            Some(Part {
                name: cells[0].replace('`', ""),
                upstream: upstream.into(),
                detail: cells[3].replace('`', ""),
                url: url.strip_suffix(')')?.into(),
            })
        })
        .collect()
}

#[derive(Default)]
pub struct LogoTaps {
    count: u8,
    last: Option<Instant>,
}

impl LogoTaps {
    pub fn push(&mut self, now: Instant) -> bool {
        if self
            .last
            .is_some_and(|last| now.saturating_duration_since(last) > Duration::from_secs(2))
        {
            self.count = 0;
        }
        self.last = Some(now);
        self.count += 1;
        if self.count == 5 {
            *self = Self::default();
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_uses_readme_upstream_links_and_integration_details() {
        let rows = parts();
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0].name, "Breeze icons");
        assert_eq!(rows[0].upstream, "KDE/breeze-icons");
        assert_eq!(rows[0].url, "https://github.com/KDE/breeze-icons");
        assert_eq!(rows[0].detail, "Bundled gear and pin icons.");
        assert!(rows.iter().any(|row| row.name == "hidapi"));
        assert!(rows
            .iter()
            .all(|row| !row.name.contains('`') && row.url.starts_with("https://github.com/")));
    }

    #[test]
    fn five_quick_logo_taps_toggle_and_a_pause_restarts_the_count() {
        let start = Instant::now();
        let mut taps = LogoTaps::default();
        assert!(!taps.push(start));
        for step in 0..4 {
            assert!(!taps.push(start + Duration::from_millis(2500 + step * 200)));
        }
        assert!(taps.push(start + Duration::from_millis(3300)));
        assert!(!taps.push(start + Duration::from_millis(3500)));
    }
}
