use kagura_interp::Host;

#[derive(Debug, Default)]
pub struct CaptureHost {
    output: String,
}

impl CaptureHost {
    pub fn output(&self) -> &str {
        &self.output
    }

    pub fn take(self) -> String {
        self.output
    }
}

impl Host for CaptureHost {
    fn print(&mut self, text: &str) {
        self.output.push_str(text);
        self.output.push('\n');
    }
}
