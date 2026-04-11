use std::{
    io::{self, IsTerminal, Write},
    time::Instant,
};

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct ProgressSpinner {
    enabled: bool,
    frame_index: usize,
    started_at: Instant,
    message: String,
}

impl ProgressSpinner {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            enabled: io::stderr().is_terminal(),
            frame_index: 0,
            started_at: Instant::now(),
            message: message.into(),
        }
    }

    pub fn tick(&mut self) {
        if !self.enabled {
            return;
        }

        let frame = SPINNER_FRAMES[self.frame_index % SPINNER_FRAMES.len()];
        self.frame_index += 1;
        let elapsed = self.started_at.elapsed().as_secs();
        eprint!("\r{} {}  {}s", frame, self.message, elapsed);
        let _ = io::stderr().flush();
    }

    pub fn success(&self, detail: &str) {
        self.finish("✓", detail);
    }

    pub fn fail(&self, detail: &str) {
        self.finish("✗", detail);
    }

    fn finish(&self, icon: &str, detail: &str) {
        if !self.enabled {
            return;
        }

        let elapsed = self.started_at.elapsed().as_secs();
        eprint!("\r{} {}  {}s", icon, detail, elapsed);
        eprintln!();
    }
}
