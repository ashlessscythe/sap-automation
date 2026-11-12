use clap::Parser;
use sap_automation::cli::Cli;
use sap_automation::utils::cli_overrides::OutputFormat;

fn parse(extra: &[&str]) -> Result<Option<OutputFormat>, clap::Error> {
    let mut args = vec!["sap_automation"];
    args.extend_from_slice(extra);
    Cli::try_parse_from(args).map(|c| c.to_overrides().expect("overrides").output_format)
}

#[test]
fn absent_flag_is_none_and_defaults_to_tsv() {
    assert_eq!(parse(&[]).unwrap(), None);
    assert_eq!(OutputFormat::default(), OutputFormat::Tsv);
}

#[test]
fn tsv_and_xlsx_are_accepted() {
    assert_eq!(
        parse(&["--output-format=tsv"]).unwrap(),
        Some(OutputFormat::Tsv)
    );
    assert_eq!(
        parse(&["--output-format", "xlsx"]).unwrap(),
        Some(OutputFormat::Xlsx)
    );
}

#[test]
fn other_values_are_rejected() {
    assert!(parse(&["--output-format=csv"]).is_err());
    assert!(parse(&["--output-format"]).is_err());
}
