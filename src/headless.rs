use crate::sink::{ObservationRecord, ObservationSink};
use std::io::{self, Write};

pub struct HeadlessSink<W: Write> {
    writer: W,
}

impl<W: Write> HeadlessSink<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W: Write> ObservationSink for HeadlessSink<W> {
    fn emit(&mut self, record: &ObservationRecord) {
        match serde_json::to_string(record) {
            Ok(json) => {
                if let Err(error) = writeln!(self.writer, "{json}") {
                    eprintln!("headless write error: {error}");
                }
            }
            Err(error) => eprintln!("headless serialization error: {error}"),
        }
    }

    fn finish(&mut self) {
        if let Err(error) = self.writer.flush() {
            eprintln!("headless flush error: {error}");
        }
    }
}

pub type BoxedHeadlessSink = HeadlessSink<Box<dyn io::Write>>;
