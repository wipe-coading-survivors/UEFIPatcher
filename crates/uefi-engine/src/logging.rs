use tracing_subscriber::fmt::format::FmtSpan;

pub fn verbosity_directive(verbose: u8, quiet: bool) -> &'static str {
    match (quiet, verbose) {
        (true, _) => "error",
        (false, 0) => "uefi_engine=info,warn",
        (false, 1) => "uefi_engine=debug,warn",
        (false, _) => "uefi_engine=trace,warn",
    }
}

pub fn span_events(verbose: u8) -> FmtSpan {
    if verbose >= 1 {
        FmtSpan::ENTER | FmtSpan::CLOSE
    } else {
        FmtSpan::NONE
    }
}

pub fn init(verbose: u8, quiet: bool) {
    let directive = verbosity_directive(verbose, quiet);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(directive));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_span_events(span_events(verbose))
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verbosity_directive_defaults() {
        assert_eq!(verbosity_directive(0, false), "uefi_engine=info,warn");
        assert_eq!(verbosity_directive(1, false), "uefi_engine=debug,warn");
        assert_eq!(verbosity_directive(2, false), "uefi_engine=trace,warn");
        assert_eq!(verbosity_directive(9, false), "uefi_engine=trace,warn");
        assert_eq!(verbosity_directive(0, true), "error");
        assert_eq!(verbosity_directive(5, true), "error");
    }

    #[test]
    fn span_events_only_when_verbose() {
        assert_eq!(span_events(0), FmtSpan::NONE);
        assert!(span_events(1) & FmtSpan::ENTER != FmtSpan::NONE);
        assert!(span_events(1) & FmtSpan::CLOSE != FmtSpan::NONE);
    }
}
