use slog::{Drain, OwnedKVList, Record, KV};
use slog_term::{
    timestamp_local, CompactFormatSerializer, CountingWriter, Decorator, RecordDecorator,
    Serializer, ThreadSafeTimestampFn,
};
use std::cell::RefCell;
use std::{io, io::Write};

pub use slog_async::Async;
pub use slog_term::TermDecorator;

pub struct PrefixedCompactFormat<D>
where
    D: Decorator,
{
    decorator: D,
    history: RefCell<Vec<(Vec<u8>, Vec<u8>)>>,
    fn_timestamp: Box<dyn ThreadSafeTimestampFn<Output = io::Result<()>>>,
    prefix: String,
}

impl<D> Drain for PrefixedCompactFormat<D>
where
    D: Decorator,
{
    type Ok = ();
    type Err = io::Error;

    fn log(&self, record: &Record<'_>, values: &OwnedKVList) -> Result<Self::Ok, Self::Err> {
        self.format_compact(record, values)
    }
}

impl<D> PrefixedCompactFormat<D>
where
    D: Decorator,
{
    pub fn new(prefix: &str, d: D) -> PrefixedCompactFormat<D> {
        Self {
            fn_timestamp: Box::new(timestamp_local),
            decorator: d,
            history: RefCell::new(vec![]),
            prefix: prefix.to_owned(),
        }
    }

    fn format_compact(&self, record: &Record<'_>, values: &OwnedKVList) -> io::Result<()> {
        self.decorator.with_record(record, values, |decorator| {
            let indent = {
                let mut history_ref = self.history.borrow_mut();
                let mut serializer = CompactFormatSerializer::new(decorator, &mut *history_ref);

                values.serialize(record, &mut serializer)?;

                serializer.finish()?
            };

            decorator.start_whitespace()?;

            for _ in 0..indent {
                write!(decorator, " ")?;
            }

            let comma_needed =
                print_msg_header(&self.prefix, &*self.fn_timestamp, decorator, record)?;
            {
                let mut serializer = Serializer::new(decorator, comma_needed, false);

                record.kv().serialize(record, &mut serializer)?;

                serializer.finish()?;
            }

            decorator.start_whitespace()?;
            writeln!(decorator)?;

            decorator.flush()?;

            Ok(())
        })
    }
}

pub fn print_msg_header(
    prefix: &str,
    fn_timestamp: &dyn ThreadSafeTimestampFn<Output = io::Result<()>>,
    mut rd: &mut dyn RecordDecorator,
    record: &Record<'_>,
) -> io::Result<bool> {
    rd.start_timestamp()?;
    fn_timestamp(&mut rd)?;

    rd.start_whitespace()?;
    write!(rd, " ")?;

    rd.start_level()?;
    write!(rd, "{}", record.level().as_short_str())?;

    rd.start_whitespace()?;
    write!(rd, " ")?;

    rd.start_msg()?;
    write!(rd, "{}:", prefix)?;

    rd.start_whitespace()?;
    write!(rd, " ")?;

    rd.start_msg()?;
    let mut count_rd = CountingWriter::new(&mut rd);
    write!(count_rd, "{}", record.msg())?;
    Ok(count_rd.count() != 0)
}
