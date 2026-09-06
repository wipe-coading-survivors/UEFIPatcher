mod client;
mod commands;
mod output;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use uefi_common::error;

#[derive(Parser)]
#[command(name = "uefi-cli", version, about = "UEFIPatcher CLI")]
struct Cli {
    #[arg(long, env = "UEFIPATCHER_SOCK")]
    sock: Option<String>,
    #[arg(long, default_value = "text", value_enum)]
    format: OutputFormatCli,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum OutputFormatCli {
    Json,
    Text,
    Tsv,
}

impl From<OutputFormatCli> for output::OutputFormat {
    fn from(c: OutputFormatCli) -> Self {
        match c {
            OutputFormatCli::Json => output::OutputFormat::Json,
            OutputFormatCli::Text => output::OutputFormat::Text,
            OutputFormatCli::Tsv => output::OutputFormat::Tsv,
        }
    }
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum ImageModeCli {
    Read,
    Write,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum InsertModeCli {
    Into,
    Before,
    After,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum SearchModeCli {
    Name,
    Utf8,
    Utf16,
    Bytes,
}

#[derive(Subcommand)]
enum Cmd {
    Session {
        #[command(subcommand)]
        sub: SessionCmd,
    },
    Image {
        #[command(subcommand)]
        sub: ImageCmd,
    },
    Node {
        #[command(subcommand)]
        sub: NodeCmd,
    },
    Artifact {
        #[command(subcommand)]
        sub: ArtifactCmd,
    },
    Hii {
        #[command(subcommand)]
        sub: HiiCmd,
    },
}

#[derive(Subcommand)]
enum SessionCmd {
    Init {
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        force: bool,
    },
    List,
    Destroy,
}

#[derive(Subcommand)]
enum ImageCmd {
    Open {
        path: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, value_enum, default_value_t = ImageModeCli::Read)]
        mode: ImageModeCli,
    },
    Switch {
        image_id: String,
    },
    Close {
        image_id: Option<String>,
    },
    Save {
        output: String,
    },
    List,
    Status,
}

#[derive(Subcommand)]
enum NodeCmd {
    List {
        #[arg(long)]
        filter: Option<String>,
        #[arg(long)]
        tree: bool,
    },
    Search {
        query: String,
        #[arg(long, value_enum, num_args = 0.., default_values_t = vec![SearchModeCli::Name])]
        mode: Vec<SearchModeCli>,
        #[arg(long, default_value = "100")]
        limit: u32,
    },
    Insert {
        target: String,
        #[arg(long, group = "source")]
        file: Option<String>,
        #[arg(long, group = "source")]
        artifact: Option<String>,
        #[arg(long, value_enum, default_value_t = InsertModeCli::Into)]
        mode: InsertModeCli,
    },
    Remove {
        target: String,
    },
    Replace {
        target: String,
        #[arg(long, group = "source")]
        file: Option<String>,
        #[arg(long, group = "source")]
        artifact: Option<String>,
        #[arg(long)]
        body_only: bool,
    },
    Rebuild {
        target: String,
    },
    Extract {
        target: String,
        #[arg(long)]
        body_only: bool,
    },
}

#[derive(Subcommand)]
enum ArtifactCmd {
    List,
    Import {
        path: String,
    },
    Export {
        artifact_id: String,
        output_path: Option<String>,
    },
}

#[derive(Subcommand)]
enum HiiCmd {
    Form {
        #[command(subcommand)]
        sub: HiiFormCmd,
    },
    #[command(name = "formset")]
    FormSet {
        #[command(subcommand)]
        sub: HiiFormSetCmd,
    },
    Question {
        #[command(subcommand)]
        sub: HiiQuestionCmd,
    },
    String {
        #[command(subcommand)]
        sub: HiiStringCmd,
    },
}

#[derive(Subcommand)]
enum HiiFormCmd {
    List,
    SetVisibility {
        form_id: String,
        #[arg(long, conflicts_with = "hidden")]
        visible: bool,
        #[arg(long)]
        hidden: bool,
    },
    Gates {
        item_id: String,
    },
    Unlock {
        item_id: String,
    },
    Add {
        #[arg(long)]
        target: String,
        #[arg(long)]
        file: String,
    },
    Hijack {
        #[arg(long)]
        target: String,
        #[arg(long)]
        file: String,
        #[arg(long)]
        setupdata_guid: Option<String>,
    },
}

#[derive(Subcommand)]
enum HiiFormSetCmd {
    Add {
        #[arg(long)]
        file: String,
        #[arg(long)]
        ffs: Option<String>,
    },
}

#[derive(Subcommand)]
enum HiiQuestionCmd {
    Gates {
        item_id: String,
    },
    Unlock {
        item_id: String,
    },
    #[command(about = "show question value map (varstore/offset/width/options)")]
    Info {
        item_id: String,
    },
    #[command(about = "seed a default value via NVAR StdDefaults stores")]
    SetValue {
        item_id: String,
        value: String,
    },
    #[command(about = "insert a OneOf question into a live form")]
    Add {
        item_id: String,
        #[arg(long)]
        file: String,
    },
}

#[derive(Subcommand)]
enum HiiStringCmd {
    List,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let format: output::OutputFormat = cli.format.into();
    let json_err = matches!(format, output::OutputFormat::Json);
    let result = dispatch(&cli, format).await;
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error::print_error(&e, json_err);
            e.exit_code().to_std()
        }
    }
}

async fn dispatch(cli: &Cli, format: output::OutputFormat) -> Result<(), error::AppError> {
    let sock = cli.sock.as_deref();
    match &cli.cmd {
        Cmd::Session { sub } => match sub {
            SessionCmd::Init { name, force } => {
                commands::session::init(name.as_deref(), sock, *force, format).await
            }
            SessionCmd::List => commands::session::list(sock, format).await,
            SessionCmd::Destroy => commands::session::destroy(sock, format).await,
        },
        Cmd::Image { sub } => match sub {
            ImageCmd::Open { path, name, mode } => {
                let mode_i = match mode {
                    ImageModeCli::Read => 0,
                    ImageModeCli::Write => 1,
                };
                commands::image::open(path, name.as_deref(), mode_i, sock, format).await
            }
            ImageCmd::Switch { image_id } => commands::image::switch(image_id, format).await,
            ImageCmd::Close { image_id } => {
                commands::image::close(image_id.as_deref(), sock, format).await
            }
            ImageCmd::Save { output } => commands::image::save(output, sock, format).await,
            ImageCmd::List => commands::image::list(sock, format).await,
            ImageCmd::Status => commands::image::status(sock, format).await,
        },
        Cmd::Node { sub } => match sub {
            NodeCmd::List { filter, tree } => {
                commands::node::list(filter.as_deref(), *tree, sock, format).await
            }
            NodeCmd::Search { query, mode, limit } => {
                let modes: Vec<i32> = mode
                    .iter()
                    .map(|m| match m {
                        SearchModeCli::Name => uefi_proto::SearchMode::Name as i32,
                        SearchModeCli::Utf8 => uefi_proto::SearchMode::Utf8 as i32,
                        SearchModeCli::Utf16 => uefi_proto::SearchMode::Utf16 as i32,
                        SearchModeCli::Bytes => uefi_proto::SearchMode::Bytes as i32,
                    })
                    .collect();
                commands::node::search(query, &modes, *limit, sock, format).await
            }
            NodeCmd::Insert {
                target,
                file,
                artifact,
                mode,
            } => {
                let mode_s = match mode {
                    InsertModeCli::Into => "into",
                    InsertModeCli::Before => "before",
                    InsertModeCli::After => "after",
                };
                commands::node::insert(
                    target,
                    file.as_deref(),
                    artifact.as_deref(),
                    mode_s,
                    sock,
                    format,
                )
                .await
            }
            NodeCmd::Remove { target } => commands::node::remove(target, sock, format).await,
            NodeCmd::Replace {
                target,
                file,
                artifact,
                body_only,
            } => {
                commands::node::replace(
                    target,
                    file.as_deref(),
                    artifact.as_deref(),
                    *body_only,
                    sock,
                    format,
                )
                .await
            }
            NodeCmd::Rebuild { target } => commands::node::rebuild(target, sock, format).await,
            NodeCmd::Extract { target, body_only } => {
                commands::node::extract(target, *body_only, sock, format).await
            }
        },
        Cmd::Artifact { sub } => match sub {
            ArtifactCmd::List => commands::artifact::list(sock, format).await,
            ArtifactCmd::Import { path } => commands::artifact::import(path, sock, format).await,
            ArtifactCmd::Export {
                artifact_id,
                output_path,
            } => {
                commands::artifact::export(artifact_id, output_path.as_deref(), sock, format).await
            }
        },
        Cmd::Hii { sub } => match sub {
            HiiCmd::Form { sub } => match sub {
                HiiFormCmd::List => commands::hii::form_list(sock, format).await,
                HiiFormCmd::SetVisibility {
                    form_id,
                    visible,
                    hidden,
                } => {
                    let vis = match (*visible, *hidden) {
                        (true, false) => true,
                        (false, true) => false,
                        _ => {
                            return Err(error::AppError::new(
                                error::ErrKind::RpcInvalidArgument,
                                "exactly one of --visible or --hidden required",
                            ));
                        }
                    };
                    commands::hii::form_set_visibility(form_id, vis, sock, format).await
                }
                HiiFormCmd::Gates { item_id } => {
                    commands::hii::form_gates(item_id, sock, format).await
                }
                HiiFormCmd::Unlock { item_id } => {
                    commands::hii::form_unlock(item_id, sock, format).await
                }
                HiiFormCmd::Add { target, file } => {
                    commands::hii::form_add(target, file, sock, format).await
                }
                HiiFormCmd::Hijack {
                    target,
                    file,
                    setupdata_guid,
                } => {
                    commands::hii::form_hijack(
                        target,
                        file,
                        setupdata_guid.as_deref(),
                        sock,
                        format,
                    )
                    .await
                }
            },
            HiiCmd::FormSet { sub } => match sub {
                HiiFormSetCmd::Add { file, ffs } => {
                    commands::hii::formset_add(file, ffs.as_deref(), sock, format).await
                }
            },
            HiiCmd::Question { sub } => match sub {
                HiiQuestionCmd::Gates { item_id } => {
                    commands::hii::question_gates(item_id, sock, format).await
                }
                HiiQuestionCmd::Unlock { item_id } => {
                    commands::hii::question_unlock(item_id, sock, format).await
                }
                HiiQuestionCmd::Info { item_id } => {
                    commands::hii::question_info(item_id, sock, format).await
                }
                HiiQuestionCmd::SetValue { item_id, value } => {
                    let v = commands::hii::parse_u64_loose(value)?;
                    commands::hii::question_set_value(item_id, v, sock, format).await
                }
                HiiQuestionCmd::Add { item_id, file } => {
                    commands::hii::question_add(item_id, file, sock, format).await
                }
            },
            HiiCmd::String { sub } => match sub {
                HiiStringCmd::List => commands::hii::string_list(sock, format).await,
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hii_formset_add_args() {
        let cli =
            Cli::try_parse_from(["uefi-cli", "hii", "formset", "add", "--file", "schema.json"])
                .unwrap();
        match cli.cmd {
            Cmd::Hii {
                sub:
                    HiiCmd::FormSet {
                        sub: HiiFormSetCmd::Add { file, ffs },
                    },
            } => {
                assert_eq!(file, "schema.json");
                assert!(ffs.is_none());
            }
            _ => panic!("expected hii formset add"),
        }
    }

    #[test]
    fn parse_hii_formset_add_with_ffs() {
        let cli = Cli::try_parse_from([
            "uefi-cli",
            "hii",
            "formset",
            "add",
            "--file",
            "s.json",
            "--ffs",
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5",
        ])
        .unwrap();
        match cli.cmd {
            Cmd::Hii {
                sub:
                    HiiCmd::FormSet {
                        sub: HiiFormSetCmd::Add { file, ffs },
                    },
            } => {
                assert_eq!(file, "s.json");
                assert_eq!(ffs.as_deref(), Some("5C60F367-A505-419A-859E-2A4FF6CA6FE5"));
            }
            _ => panic!("expected hii formset add"),
        }
    }

    #[test]
    fn parse_hii_form_gates_and_unlock() {
        let cli =
            Cli::try_parse_from(["uefi-cli", "hii", "form", "gates", "g:0x10:0#10029"]).unwrap();
        match cli.cmd {
            Cmd::Hii {
                sub:
                    HiiCmd::Form {
                        sub: HiiFormCmd::Gates { item_id },
                    },
            } => assert_eq!(item_id, "g:0x10:0#10029"),
            _ => panic!("expected hii form gates"),
        }
        let cli = Cli::try_parse_from([
            "uefi-cli",
            "hii",
            "question",
            "unlock",
            "g:0x10:0#10029:0x3B",
        ])
        .unwrap();
        match cli.cmd {
            Cmd::Hii {
                sub:
                    HiiCmd::Question {
                        sub: HiiQuestionCmd::Unlock { item_id },
                    },
            } => assert_eq!(item_id, "g:0x10:0#10029:0x3B"),
            _ => panic!("expected hii question unlock"),
        }
    }

    #[test]
    fn parse_hii_question_info_and_set_value() {
        let cli =
            Cli::try_parse_from(["uefi-cli", "hii", "question", "info", "0#10029:0x3B"]).unwrap();
        match cli.cmd {
            Cmd::Hii {
                sub:
                    HiiCmd::Question {
                        sub: HiiQuestionCmd::Info { item_id },
                    },
            } => assert_eq!(item_id, "0#10029:0x3B"),
            _ => panic!("expected hii question info"),
        }
        let cli = Cli::try_parse_from([
            "uefi-cli",
            "hii",
            "question",
            "set-value",
            "0#10029:0x3B",
            "0x1",
        ])
        .unwrap();
        match cli.cmd {
            Cmd::Hii {
                sub:
                    HiiCmd::Question {
                        sub: HiiQuestionCmd::SetValue { item_id, value },
                    },
            } => {
                assert_eq!(item_id, "0#10029:0x3B");
                assert_eq!(value, "0x1");
                assert_eq!(commands::hii::parse_u64_loose(&value).unwrap(), 1);
            }
            _ => panic!("expected hii question set-value"),
        }
    }

    #[test]
    fn parse_hii_form_add_args() {
        let cli = Cli::try_parse_from([
            "uefi-cli",
            "hii",
            "form",
            "add",
            "--target",
            "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1",
            "--file",
            "schema.json",
        ])
        .unwrap();
        match cli.cmd {
            Cmd::Hii {
                sub:
                    HiiCmd::Form {
                        sub: HiiFormCmd::Add { target, file },
                    },
            } => {
                assert_eq!(target, "5C60F367-A505-419A-859E-2A4FF6CA6FE5:0x19:1");
                assert_eq!(file, "schema.json");
            }
            _ => panic!("expected hii form add"),
        }
    }

    #[test]
    fn parse_hii_question_add_args() {
        let cli = Cli::try_parse_from([
            "uefi-cli",
            "hii",
            "question",
            "add",
            "3/28/1/0#10019",
            "--file",
            "s3_questions.json",
        ])
        .unwrap();
        match cli.cmd {
            Cmd::Hii {
                sub:
                    HiiCmd::Question {
                        sub: HiiQuestionCmd::Add { item_id, file },
                    },
            } => {
                assert_eq!(item_id, "3/28/1/0#10019");
                assert_eq!(file, "s3_questions.json");
            }
            _ => panic!("expected hii question add"),
        }
    }

    #[test]
    fn parse_hii_form_hijack() {
        let cli = Cli::try_parse_from([
            "uefi-cli",
            "hii",
            "form",
            "hijack",
            "--target",
            "899407D7-99FE-43D8-9A21-79EC328CAC21:0x10:0#10029",
            "--file",
            "schema.json",
            "--setupdata-guid",
            "FE612B72-203C-47B1-856D946EB371",
        ])
        .unwrap();
        match cli.cmd {
            Cmd::Hii {
                sub:
                    HiiCmd::Form {
                        sub:
                            HiiFormCmd::Hijack {
                                target,
                                file,
                                setupdata_guid,
                            },
                    },
            } => {
                assert_eq!(target, "899407D7-99FE-43D8-9A21-79EC328CAC21:0x10:0#10029");
                assert_eq!(file, "schema.json");
                assert_eq!(
                    setupdata_guid.as_deref(),
                    Some("FE612B72-203C-47B1-856D946EB371")
                );
            }
            _ => panic!("expected hii form hijack"),
        }
    }
}
