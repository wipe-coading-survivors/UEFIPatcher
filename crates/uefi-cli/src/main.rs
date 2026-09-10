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
    #[command(about = "manage engine sessions")]
    Session {
        #[command(subcommand)]
        sub: SessionCmd,
    },
    #[command(about = "open and manage BIOS images on the engine")]
    Image {
        #[command(subcommand)]
        sub: ImageCmd,
    },
    #[command(about = "inspect and edit firmware tree nodes")]
    Node {
        #[command(subcommand)]
        sub: NodeCmd,
    },
    #[command(about = "manage stored artifacts")]
    Artifact {
        #[command(subcommand)]
        sub: ArtifactCmd,
    },
    #[command(about = "read and edit HII forms, strings and questions")]
    Hii {
        #[command(subcommand)]
        sub: HiiCmd,
    },
}

#[derive(Subcommand)]
enum SessionCmd {
    #[command(about = "create a session and write client state")]
    Init {
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        force: bool,
    },
    #[command(about = "list active sessions")]
    List,
    #[command(about = "destroy the session and remove client state")]
    Destroy,
}

#[derive(Subcommand)]
enum ImageCmd {
    #[command(about = "open a BIOS image from the server filesystem")]
    Open {
        path: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, value_enum, default_value_t = ImageModeCli::Read)]
        mode: ImageModeCli,
    },
    #[command(about = "set the active image for subsequent commands")]
    Switch { image_id: String },
    #[command(about = "close an opened image")]
    Close { image_id: Option<String> },
    #[command(about = "write the active image to a file on the server")]
    Save { output: String },
    #[command(about = "list images opened in the session")]
    List,
    #[command(about = "show details of the active image")]
    Status,
}

#[derive(Subcommand)]
enum NodeCmd {
    #[command(about = "list nodes of the active image")]
    List {
        #[arg(long)]
        filter: Option<String>,
        #[arg(long)]
        tree: bool,
    },
    #[command(about = "search nodes by name or data")]
    Search {
        query: String,
        #[arg(long, value_enum, num_args = 0.., default_values_t = vec![SearchModeCli::Name])]
        mode: Vec<SearchModeCli>,
        #[arg(long, default_value = "100")]
        limit: u32,
    },
    #[command(about = "insert a file or artifact as a new node")]
    Insert {
        target: String,
        #[arg(
            long,
            group = "source",
            help = "read the new node body from a server-side file"
        )]
        file: Option<String>,
        #[arg(
            long,
            group = "source",
            help = "reuse a stored artifact as the new node body"
        )]
        artifact: Option<String>,
        #[arg(long, value_enum, default_value_t = InsertModeCli::Into, help = "placement relative to the target")]
        mode: InsertModeCli,
    },
    #[command(about = "remove a node")]
    Remove { target: String },
    #[command(about = "replace a node body from a file or artifact")]
    Replace {
        target: String,
        #[arg(
            long,
            group = "source",
            help = "read the new node body from a server-side file"
        )]
        file: Option<String>,
        #[arg(
            long,
            group = "source",
            help = "reuse a stored artifact as the new node body"
        )]
        artifact: Option<String>,
        #[arg(long, help = "replace the body only, keep the node header")]
        body_only: bool,
    },
    #[command(about = "rebuild a node from its children")]
    Rebuild { target: String },
    #[command(about = "extract a node body into a new artifact")]
    Extract {
        target: String,
        #[arg(long, help = "extract the body without the section header")]
        body_only: bool,
    },
}

#[derive(Subcommand)]
enum ArtifactCmd {
    #[command(about = "list stored artifacts")]
    List,
    #[command(about = "import a local file as an artifact")]
    Import { path: String },
    #[command(about = "export an artifact to the server filesystem")]
    Export {
        artifact_id: String,
        output_path: Option<String>,
    },
}

#[derive(Subcommand)]
enum HiiCmd {
    #[command(about = "form-level HII operations")]
    Form {
        #[command(subcommand)]
        sub: HiiFormCmd,
    },
    #[command(name = "formset", about = "formset-level HII operations")]
    FormSet {
        #[command(subcommand)]
        sub: HiiFormSetCmd,
    },
    #[command(about = "question-level HII operations")]
    Question {
        #[command(subcommand)]
        sub: HiiQuestionCmd,
    },
    #[command(about = "$SPF page-table operations")]
    Page {
        #[command(subcommand)]
        sub: HiiPageCmd,
    },
    #[command(about = "HII string operations")]
    String {
        #[command(subcommand)]
        sub: HiiStringCmd,
    },
}

#[derive(Subcommand)]
enum HiiFormCmd {
    #[command(about = "list HII forms")]
    List,
    #[command(about = "show or hide a form via its suppress scope")]
    SetVisibility {
        form_id: String,
        #[arg(long, conflicts_with = "hidden")]
        visible: bool,
        #[arg(long)]
        hidden: bool,
    },
    #[command(about = "list gates (suppress/grayout) guarding a form")]
    Gates { item_id: String },
    #[command(about = "flip gates to unlock a form")]
    Unlock { item_id: String },
    #[command(about = "insert a form from a schema file into a live formset")]
    Add {
        #[arg(long)]
        target: String,
        #[arg(long)]
        file: String,
    },
    #[command(about = "replace a form via AMI SetupData hijack")]
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
    #[command(about = "append a new formset from a schema file")]
    Add {
        #[arg(long)]
        file: String,
        #[arg(long)]
        ffs: Option<String>,
    },
}

#[derive(Subcommand)]
enum HiiQuestionCmd {
    #[command(about = "list gates (suppress/grayout) guarding a question")]
    Gates { item_id: String },
    #[command(about = "flip gates to unlock a question")]
    Unlock { item_id: String },
    #[command(about = "show question value map (varstore/offset/width/options)")]
    Info { item_id: String },
    #[command(about = "seed a default value via NVAR StdDefaults stores")]
    SetValue { item_id: String, value: String },
    #[command(about = "insert a OneOf question into a live form")]
    Add {
        item_id: String,
        #[arg(long)]
        file: String,
    },
}

#[derive(Subcommand)]
enum HiiPageCmd {
    #[command(
        about = "add a form to the $SPF page table (no IFR validation; create the form first)"
    )]
    Add {
        target: String,
        #[arg(long)]
        file: String,
    },
}

#[derive(Subcommand)]
enum HiiStringCmd {
    #[command(about = "list HII strings")]
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
            ImageCmd::Switch { image_id } => commands::image::switch(image_id, sock, format).await,
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
            HiiCmd::Page { sub } => match sub {
                HiiPageCmd::Add { target, file } => {
                    commands::hii::page_add(target, file, sock, format).await
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
    fn parse_hii_page_add_args() {
        let cli = Cli::try_parse_from([
            "uefi-cli",
            "hii",
            "page",
            "add",
            "3/28/1/0#10019",
            "--file",
            "np_page.json",
        ])
        .unwrap();
        match cli.cmd {
            Cmd::Hii {
                sub:
                    HiiCmd::Page {
                        sub: HiiPageCmd::Add { target, file },
                    },
            } => {
                assert_eq!(target, "3/28/1/0#10019");
                assert_eq!(file, "np_page.json");
            }
            _ => panic!("expected hii page add"),
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
