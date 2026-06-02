use crate::sink::{ObservationRecord, ObservationSink};
use std::io::{self, Write};

pub struct HeadlessSink<W: Write> {
    writer: W,
    closed: bool,
}

impl<W: Write> HeadlessSink<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            closed: false,
        }
    }
}

impl<W: Write> ObservationSink for HeadlessSink<W> {
    fn emit(&mut self, record: &ObservationRecord) {
        if self.closed {
            return;
        }

        match serde_json::to_string(record) {
            Ok(json) => {
                if let Err(error) = writeln!(self.writer, "{json}") {
                    if error.kind() == io::ErrorKind::BrokenPipe {
                        self.closed = true;
                    } else {
                        eprintln!("headless write error: {error}");
                    }
                }
            }
            Err(error) => eprintln!("headless serialization error: {error}"),
        }
    }

    fn finish(&mut self) {
        if self.closed {
            return;
        }

        if let Err(error) = self.writer.flush() {
            if error.kind() != io::ErrorKind::BrokenPipe {
                eprintln!("headless flush error: {error}");
            }
        }
    }
}

pub type BoxedHeadlessSink = HeadlessSink<Box<dyn io::Write>>;
