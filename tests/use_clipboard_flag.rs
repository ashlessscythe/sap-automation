use clap::Parser;
use sap_automation::cli::Cli;

fn parse(extra: &[&str]) -> Option<bool> {
    let mut args = vec!["sap_automation"];
    args.extend_from_slice(extra);
    Cli::try_parse_from(args)
        .expect("parse")
        .to_overrides()
        .expect("overrides")
        .use_clipboard
}

#[test]
fn absent_flag_means_off() {
    assert_eq!(parse(&[]), None);
}

#[test]
fn bare_flag_and_truthy_values_enable() {
    assert_eq!(parse(&["--use-clipboard"]), Some(true));
    assert_eq!(parse(&["--use-clipboard=true"]), Some(true));
    assert_eq!(parse(&["--use-clipboard=1"]), Some(true));
}

#[test]
fn falsy_values_disable() {
    assert_eq!(parse(&["--use-clipboard=false"]), Some(false));
    assert_eq!(parse(&["--use-clipboard=0"]), Some(false));
}
