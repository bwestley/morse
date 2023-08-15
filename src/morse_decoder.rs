use std::time::SystemTime;

use serde::{Deserialize, Serialize};

pub enum Code {
    Dit,
    Dah,
    Short,
    Long,
}

impl Code {
    pub fn display_code_string(code_string: Vec<Code>) -> String {
        code_string
            .iter()
            .filter_map(|code| match code {
                Code::Dit => Some('.'),
                Code::Dah => Some('-'),
                Code::Short => Some(' '),
                Code::Long => Some('\n'),
            })
            .collect()
    }
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct DecoderSettings {
    pub dit_dah: u64,
    pub letter: u64,
    pub letter_word: u64,
}

impl Default for DecoderSettings {
    fn default() -> Self {
        Self {
            dit_dah: 300,
            letter: 500,
            letter_word: 2000,
        }
    }
}

pub struct MorseDecoder {
    ring: [(u64, bool); 128],
    index: usize,
    last_time: SystemTime,
    last_on: bool,
}

impl MorseDecoder {
    pub const LENGTH: usize = 128;

    pub fn new() -> Self {
        Self {
            ring: [(u64::MAX, false); 128],
            index: 0,
            last_time: SystemTime::now(),
            last_on: false,
        }
    }

    pub fn display(&self) -> String {
        let mut text = String::new();
        let mut mark_times: Vec<u64> = self
            .ring
            .iter()
            .filter_map(|x| (x.0 != u64::MAX && !x.1).then_some(x.0))
            .collect();
        mark_times.sort();
        mark_times.reverse();
        let marks = mark_times.len();
        let mut gap_times: Vec<u64> = self
            .ring
            .iter()
            .filter_map(|x| (x.0 != u64::MAX && x.1).then_some(x.0))
            .collect();
        gap_times.sort();
        gap_times.reverse();
        let gaps = gap_times.len();

        text += "Durations (ms)\nMarks Gaps\n----- -----";
        for i in 0..marks.min(gaps) {
            text += &format!("\n{:05} {:05}", mark_times[i], gap_times[i]);
        }
        if marks > gaps {
            for i in gaps..marks {
                text += &format!("\n{:05}", mark_times[i]);
            }
        } else {
            for i in marks..gaps {
                text += &format!("\n      {:05}", gap_times[i]);
            }
        }
        text
    }

    pub fn tick(&mut self, on: bool) {
        let now = SystemTime::now();
        if self.last_on != on {
            self.index = (self.index + 1) % Self::LENGTH;
            self.ring[self.index] = (
                now.duration_since(self.last_time).unwrap().as_millis() as u64,
                on,
            );
            self.last_on = on;
            self.last_time = now;
        }
    }

    pub fn reset(&mut self) {
        self.ring.fill((u64::MAX, false));
        self.index = 0;
        self.last_time = SystemTime::now();
        self.last_on = false;
    }

    pub fn decode(&self, settings: &DecoderSettings) -> Vec<Code> {
        let mut code: Vec<Code> = Vec::new();
        for i in (((self.index + 1) % Self::LENGTH)..Self::LENGTH).chain(0..=self.index) {
            let (duration, on) = self.ring[i];
            if duration == u64::MAX {
                continue;
            }
            if on {
                // Signal was switched on, duration is the preceding off period.
                if duration < settings.letter {
                    // Intra-character gap between dit and dah
                } else if duration < settings.letter_word {
                    code.push(Code::Short) // Short gap between letters
                } else {
                    code.push(Code::Long) // Medium gap between words
                }
            } else {
                // Signal was switched off, duration is the preceding on period.
                if duration < settings.dit_dah {
                    code.push(Code::Dit) // Short mark, dit
                } else {
                    code.push(Code::Dah) // Longer mark, dah
                }
            }
        }
        code
    }
}
