use std::fmt;
use tracing::{
    Event, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    field::VisitOutput,
    fmt::{
        FmtContext, FormatEvent, FormatFields, FormattedFields,
        format::{DefaultVisitor, Writer},
        time::{FormatTime, SystemTime},
    },
    registry::LookupSpan,
};

pub struct SourceFormat;

impl<S, N> FormatEvent<S, N> for SourceFormat
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let metadata = event.metadata();
        if metadata.target() != "api::error" {
            return tracing_subscriber::fmt::format().format_event(ctx, writer, event);
        }
        let mut fields = String::new();
        let mut visitor = FailureFields {
            fields: DefaultVisitor::new(Writer::new(&mut fields), true),
            origin: None,
            status: None,
        };
        event.record(&mut visitor);
        visitor.fields.finish()?;
        let (Some(origin), Some(status)) = (visitor.origin, visitor.status) else {
            return tracing_subscriber::fmt::format().format_event(ctx, writer, event);
        };
        SystemTime.format_time(&mut writer)?;
        write!(writer, " {} {origin}: [{status}] ", metadata.level())?;
        if let Some(scope) = ctx.event_scope() {
            for span in scope.from_root() {
                write!(writer, "{}", span.name())?;
                let extensions = span.extensions();
                if let Some(fields) = extensions.get::<FormattedFields<N>>()
                    && !fields.is_empty()
                {
                    write!(writer, "{{{fields}}}")?;
                }
                write!(writer, ": ")?;
            }
        }
        writeln!(writer, "{fields}")
    }
}

struct FailureFields<'a> {
    fields: DefaultVisitor<'a>,
    origin: Option<String>,
    status: Option<String>,
}

impl Visit for FailureFields<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        match field.name() {
            "origin" => self.origin = Some(format!("{value:?}")),
            "status" => self.status = Some(format!("{value:?}")),
            _ => self.fields.record_debug(field, value),
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "origin" {
            self.origin = Some(value.to_owned());
        } else {
            self.fields.record_str(field, value);
        }
    }
}
