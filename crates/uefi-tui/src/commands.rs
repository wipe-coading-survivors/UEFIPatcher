use http::Uri;
use hyper_util::rt::TokioIo;
use tonic::Request;
use tonic::transport::{Channel, Endpoint};
use uefi_common::cli::{Source, parse_node_flags};
use uefi_common::state::{State, resolve_sock};
use uefi_proto::engine_service_client::EngineServiceClient;
use uefi_proto::*;

use crate::app::{App, View};

pub struct Client {
    inner: EngineServiceClient<Channel>,
    pub state: State,
}

fn auth_req<T>(state: &State, body: T) -> Request<T> {
    let mut req = Request::new(body);
    if let (Some(sid), Some(tok)) = (&state.session_id, &state.token) {
        req.metadata_mut()
            .insert("authorization", format!("Bearer {tok}").parse().unwrap());
        req.metadata_mut()
            .insert("x-session-id", sid.parse().unwrap());
    }
    req
}

pub async fn connect(cli_sock: Option<&str>, state: State) -> Result<Client, String> {
    let sock = resolve_sock(cli_sock, &state);
    let sock_str = sock.display().to_string();
    let channel = Endpoint::try_from("http://localhost")
        .map_err(|e| e.to_string())?
        .connect_with_connector(tower::service_fn(move |_: Uri| {
            let s = sock_str.clone();
            async move {
                Ok::<_, std::io::Error>(TokioIo::new(tokio::net::UnixStream::connect(s).await?))
            }
        }))
        .await
        .map_err(|e| e.to_string())?;
    Ok(Client {
        inner: EngineServiceClient::new(channel).max_encoding_message_size(64 * 1024 * 1024),
        state,
    })
}

impl Client {
    pub async fn image_upload(
        &mut self,
        session_id: &str,
        data: Vec<u8>,
        mode: i32,
        name: &str,
    ) -> Result<ImageOpenResponse, String> {
        self.inner
            .image_upload(auth_req(
                &self.state,
                ImageUploadRequest {
                    session_id: session_id.into(),
                    data,
                    mode,
                    name: name.into(),
                },
            ))
            .await
            .map_err(|e| e.message().to_string())
            .map(|r| r.into_inner())
    }

    pub async fn image_snapshot_create(
        &mut self,
        image_id: &str,
        name: &str,
    ) -> Result<ImageSnapshotCreateResponse, String> {
        self.inner
            .image_snapshot_create(auth_req(
                &self.state,
                ImageSnapshotCreateRequest {
                    image_id: image_id.into(),
                    name: name.into(),
                },
            ))
            .await
            .map_err(|e| e.message().to_string())
            .map(|r| r.into_inner())
    }

    pub async fn image_snapshots_list(
        &mut self,
        image_id: &str,
    ) -> Result<Vec<ImageSnapshotInfo>, String> {
        self.inner
            .image_snapshots_list(auth_req(
                &self.state,
                ImageSnapshotsListRequest {
                    image_id: image_id.into(),
                },
            ))
            .await
            .map_err(|e| e.message().to_string())
            .map(|r| r.into_inner().snapshots)
    }

    pub async fn image_snapshot_restore(
        &mut self,
        image_id: &str,
        snapshot_id: &str,
    ) -> Result<Empty, String> {
        self.inner
            .image_snapshot_restore(auth_req(
                &self.state,
                ImageSnapshotRestoreRequest {
                    image_id: image_id.into(),
                    snapshot_id: snapshot_id.into(),
                },
            ))
            .await
            .map_err(|e| e.message().to_string())
            .map(|r| r.into_inner())
    }
}

fn mode_to_i32(s: &str) -> Result<i32, String> {
    match s {
        "into" | "" => Ok(0),
        "before" => Ok(1),
        "after" => Ok(2),
        _ => Err(format!("unknown mode: {s} (into|before|after)")),
    }
}

const NVAR_USAGE: &str = "usage: :nvar list [PATH] [--var NAME] | :nvar set NAME --offset OFF --value VAL [--guid GUID] [--width 1|2|4|8]";

fn flag_value<'a>(parts: &'a [&str], flag: &str) -> Option<&'a str> {
    parts
        .iter()
        .position(|p| *p == flag)
        .and_then(|i| parts.get(i + 1).copied())
        .filter(|v| !v.starts_with("--"))
}

struct NvarSetArgs {
    name: String,
    guid: Option<String>,
    offset: u64,
    value: u64,
    width: u32,
}

/// Парсинг `:nvar set NAME --offset OFF --value VAL [--guid GUID]
/// [--width 1|2|4|8]`; offset/value — dec или 0x-hex (parse_u64_loose),
/// width дефолт 1, как в CLI (uefi-cli NvarCmd::Set).
fn parse_nvar_set(parts: &[&str]) -> Result<NvarSetArgs, String> {
    const USAGE: &str =
        "usage: :nvar set NAME --offset OFF --value VAL [--guid GUID] [--width 1|2|4|8]";
    let name = parts.get(2).filter(|p| !p.starts_with("--")).ok_or(USAGE)?;
    let offset = parse_u64_loose(flag_value(parts, "--offset").ok_or(USAGE)?)?;
    let value = parse_u64_loose(flag_value(parts, "--value").ok_or(USAGE)?)?;
    let guid = flag_value(parts, "--guid").map(str::to_string);
    let width = match flag_value(parts, "--width") {
        Some(w) => w
            .parse::<u32>()
            .map_err(|_| "width must be one of 1, 2, 4, 8".to_string())?,
        None => 1,
    };
    if !matches!(width, 1 | 2 | 4 | 8) {
        return Err("width must be one of 1, 2, 4, 8".into());
    }
    let mut i = 3;
    while i < parts.len() {
        if matches!(parts[i], "--offset" | "--value" | "--guid" | "--width") {
            i += 2;
        } else {
            return Err(format!("unexpected argument {} ({USAGE})", parts[i]));
        }
    }
    Ok(NvarSetArgs {
        name: name.to_string(),
        guid,
        offset,
        value,
        width,
    })
}

/// Сводный статус `:nvar list` без PATH: число сторов/переменных и их пути
/// (полный листинг движка пропускает копии за барьером — подсказка PATH).
fn nvar_summary_status(stores: &[uefi_proto::NvarStoreInfo]) -> String {
    if stores.is_empty() {
        return "nvar: no stores (behind-barrier copy? :nvar list PATH)".into();
    }
    let vars: usize = stores.iter().map(|s| s.vars.len()).sum();
    let paths: Vec<&str> = stores.iter().map(|s| s.path.as_str()).collect();
    format!(
        "nvar: {} stores · {} vars ({})",
        stores.len(),
        vars,
        paths.join(" · ")
    )
}

/// Строки переменной для `--var NAME`: `имя офсет размер стор` (офсет —
/// абсолютный офсет данных, как в выводе CLI nvar list).
fn nvar_var_rows(stores: &[uefi_proto::NvarStoreInfo], name: &str) -> Vec<String> {
    stores
        .iter()
        .flat_map(|s| {
            s.vars
                .iter()
                .filter(|v| v.name == name)
                .map(|v| format!("{} {:#010x} {}B @{}", v.name, v.offset, v.size, s.path))
        })
        .collect()
}

/// Prefill `:nvar set` из переменной под курсором NVAR-панель (offset 0 =
/// начало данных переменной; guid подставляется, если запись его несёт).
pub fn nvar_set_prefill(app: &App) -> Option<String> {
    let v = app.nvar_vars().get(app.nvar.cursor)?;
    let guid = if v.guid.is_empty() {
        String::new()
    } else {
        format!("--guid {} ", v.guid)
    };
    Some(format!("nvar set {} {guid}--offset 0 --value ", v.name))
}

fn parse_u64_loose(s: &str) -> Result<u64, String> {
    if let Some(hex) = s.strip_prefix("0x") {
        u64::from_str_radix(hex, 16)
    } else {
        s.parse::<u64>()
    }
    .map_err(|e| format!("invalid value '{s}': {e}"))
}

/// Относительный путь :save абсолютизируется от CWD клиента: файл пишет
/// engine-процесс, и без этого путь резолвится от его каталога запуска.
fn absolutize_path(raw: &str) -> String {
    let p = std::path::Path::new(raw);
    if p.is_absolute() {
        return raw.to_string();
    }
    match std::env::current_dir() {
        Ok(cwd) => cwd.join(p).display().to_string(),
        Err(_) => raw.to_string(),
    }
}

/// PATH для :export — absolutize либо cwd/<artifact_id> (паритет CLI,
/// resolve_output_path uefi-cli). Спека R5: чинит дефолт-каталог.
fn export_output_path(arg: Option<&str>, artifact_id: &str) -> String {
    match arg {
        Some(p) => absolutize_path(p),
        None => match std::env::current_dir() {
            Ok(cwd) => cwd.join(artifact_id).display().to_string(),
            Err(_) => artifact_id.to_string(),
        },
    }
}

/// Читает schema-файл в TUI-процессе (клиент): в RPC уходит содержимое
/// строкой, путь до движка не доходит (не :save — там пишет engine).
fn read_schema(file: &str) -> Result<String, String> {
    std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))
}

/// Конверт-маршрутизация `:hii form add` (спека hii-form-export §4/§5,
/// паритет CLI ensure_bare_schema uefi-cli/src/commands/hii.rs:151): файл
/// с верхнеуровневой мета/refs — пакет `:hii import`, здесь отвергается.
/// Признак конверта: bare-файл `split_envelope` пропускает байт-в-байт;
/// InvalidJson уходит прежним путём — валидирует движок.
fn ensure_bare_schema(schema_json: &str) -> Result<(), String> {
    let is_package = match uefi_common::envelope::split_envelope(schema_json) {
        Ok(env) => env.body != schema_json,
        Err(uefi_common::envelope::EnvelopeError::InvalidJson(_)) => false,
        Err(_) => true,
    };
    if is_package {
        return Err("package file: use hii import".to_string());
    }
    Ok(())
}

/// Спека hii-form-export §2/§4: parent_form_id=0 — корневая форма, refs-секции
/// нет; иначе entries — prompt/help GOTO родителя из ответа RPC (question_id
/// сознательно не пишется — коллизия при реимпорте в тот же образ).
/// u32→u16 с явной ошибкой — значение больше u16 означает битый ответ движка.
fn refs_from_parent(
    parent_form_id: u32,
    parent_entries: &[uefi_proto::HiiFormExportEntry],
) -> Result<Option<uefi_common::envelope::RefsSection>, String> {
    if parent_form_id == 0 {
        return Ok(None);
    }
    let id = u16::try_from(parent_form_id)
        .map_err(|_| format!("parent_form_id {parent_form_id} does not fit u16"))?;
    Ok(Some(uefi_common::envelope::RefsSection {
        parent_form_id: id,
        entries: parent_entries
            .iter()
            .map(|e| uefi_common::envelope::RefEntry {
                prompt: Some(e.prompt.clone()),
                help: Some(e.help.clone()),
                ..uefi_common::envelope::RefEntry::default()
            })
            .collect(),
    }))
}

/// Дефолт-путь :hii form export без --out (stdout в TUI нет):
/// $TMPDIR/uefipatcher-export-<form_id>.json, путь показывается в статусе.
/// form_id — часть item_id после `#` (до `:qid`), без неё — "form".
fn export_default_path(item_id: &str) -> String {
    let dir = std::env::var("TMPDIR").unwrap_or_default();
    export_default_path_in(&dir, item_id)
}

fn export_default_path_in(dir: &str, item_id: &str) -> String {
    let dir = if dir.is_empty() { "/tmp" } else { dir };
    let fid = item_id
        .rsplit_once('#')
        .map(|(_, d)| d.split_once(':').map_or(d, |(f, _)| f))
        .unwrap_or("form");
    format!("{dir}/uefipatcher-export-{fid}.json")
}

/// Pre-check §3.1: родитель refs-секции существует в целевом формсете.
fn check_parent(forms: &[uefi_proto::FormInfo], target: &str, parent: u16) -> Result<(), String> {
    if forms
        .iter()
        .any(|f| f.form_id.eq_ignore_ascii_case(target) && f.form_id_ifr == parent as u32)
    {
        Ok(())
    } else {
        Err(format!(
            "parent form {parent} not found in target '{target}'; see 'hii form list'"
        ))
    }
}

/// Pre-check §3.1: авторские question_id из entries не заняты в родителе.
fn check_qids(refs: &uefi_common::envelope::RefsSection, busy: &[u16]) -> Result<(), String> {
    for qid in refs.entries.iter().filter_map(|e| e.question_id) {
        if busy.contains(&qid) {
            return Err(format!(
                "question_id {qid:#06x} already busy in parent form {}",
                refs.parent_form_id
            ));
        }
    }
    Ok(())
}

/// Pre-check §3.1 (анти-dangling): явные form_id-таргеты entries существуют —
/// с formset_guid в том формсете (кросс-формсетный REF3), без — в целевом.
fn check_targets(
    forms: &[uefi_proto::FormInfo],
    target: &str,
    refs: &uefi_common::envelope::RefsSection,
) -> Result<(), String> {
    for e in &refs.entries {
        let Some(id) = e.form_id else { continue };
        let (found, err) = match e.formset_guid.as_deref() {
            Some(guid) => (
                forms.iter().any(|f| {
                    f.form_id_ifr == id as u32 && f.formset_guid.eq_ignore_ascii_case(guid)
                }),
                format!("refs target form {id} not found in formset {guid}"),
            ),
            None => (
                forms
                    .iter()
                    .any(|f| f.form_id_ifr == id as u32 && f.form_id.eq_ignore_ascii_case(target)),
                format!("refs target form {id} not found in target '{target}'"),
            ),
        };
        if !found {
            return Err(err);
        }
    }
    Ok(())
}

fn fit_u16(v: u32, what: &str) -> Result<u16, String> {
    u16::try_from(v).map_err(|_| format!("{what} {v} does not fit u16"))
}

fn busy_qids(questions: &[uefi_proto::QuestionSummary]) -> Result<Vec<u16>, String> {
    questions
        .iter()
        .map(|q| fit_u16(q.question_id, "question_id"))
        .collect()
}

fn varstore_briefs(
    stores: &[uefi_proto::VarStoreInfo],
) -> Result<Vec<uefi_common::envelope::VarstoreBrief>, String> {
    stores
        .iter()
        .map(|v| {
            Ok(uefi_common::envelope::VarstoreBrief {
                id: fit_u16(v.id, "varstore id")?,
                guid: v.guid.clone(),
                size: fit_u16(v.size, "varstore size")?,
                name: v.name.clone(),
            })
        })
        .collect()
}

/// :hii form export (спека hii-form-export §4): RPC отдаёт bare-тело и
/// факты, конверт (meta.source/refs/meta.lossy) собирает TUI-клиент;
/// файл пишет TUI-процесс — `--out` либо дефолт $TMPDIR (stdout в TUI нет).
async fn hii_form_export(
    app: &mut App,
    client: &mut Client,
    image_id: &str,
    item_id: &str,
    out: Option<&str>,
) -> Result<String, String> {
    let resp = client
        .inner
        .hii_form_export(auth_req(
            &client.state,
            HiiFormExportRequest {
                image_id: image_id.into(),
                item_id: item_id.into(),
            },
        ))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    let refs = refs_from_parent(resp.parent_form_id, &resp.parent_entries)?;
    let envelope = uefi_common::envelope::wrap_export(
        &resp.schema_json,
        Some(uefi_common::envelope::SourceMeta {
            formset_guid: resp.formset_guid,
        }),
        refs,
        resp.lossy.clone(),
    );
    let path = match out {
        Some(p) => absolutize_path(p),
        None => export_default_path(item_id),
    };
    std::fs::write(&path, format!("{envelope}\n")).map_err(|e| format!("{path}: {e}"))?;
    let lossy = if resp.lossy.is_empty() {
        String::new()
    } else {
        format!(" · lossy {}", resp.lossy.join(","))
    };
    app.status_msg = format!("form exported to {path}{lossy}");
    Ok(path)
}

/// :hii import (спека hii-form-export §3) — тот же порядок, что CLI-импорт:
/// pre-check (родитель/qid/таргеты, до мутаций) → plan_varstores → form add
/// → plan_ref_step → question add `<target>#<parent>` с {"refs":[…]}-wire.
/// Планировщики — uefi-common, RPC — клиент TUI; refs-only пакеты (body
/// пуст) пропускают form add; падение ref-фазы после form-фазы уходит в
/// двухфазный статус (автоотката нет, §3.4); warn чужого формсета — в
/// статус (stderr в TUI нет).
async fn hii_import(
    app: &mut App,
    client: &mut Client,
    image_id: &str,
    target: &str,
    file: &str,
) -> Result<String, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{file}: {e}"))?;
    let env = uefi_common::envelope::split_envelope(&text).map_err(|e| e.to_string())?;

    let mut busy: Vec<u16> = Vec::new();
    let mut warn = String::new();
    if let Some(refs) = env.refs.as_ref() {
        let forms = client
            .inner
            .hii_list_forms(auth_req(
                &client.state,
                HiiListFormsRequest {
                    image_id: image_id.into(),
                },
            ))
            .await
            .map_err(|e| e.message().to_string())?
            .into_inner()
            .forms;
        check_parent(&forms, target, refs.parent_form_id)?;
        let questions = client
            .inner
            .hii_list_questions(auth_req(
                &client.state,
                HiiListQuestionsRequest {
                    image_id: image_id.into(),
                    target: target.into(),
                    form_id: refs.parent_form_id as u32,
                },
            ))
            .await
            .map_err(|e| e.message().to_string())?
            .into_inner()
            .questions;
        busy = busy_qids(&questions)?;
        check_qids(refs, &busy)?;
        check_targets(&forms, target, refs)?;
        let target_formset = forms
            .iter()
            .find(|f| f.form_id.eq_ignore_ascii_case(target))
            .map(|f| f.formset_guid.clone());
        if let (Some(src), Some(actual)) = (env.meta.source.as_ref(), target_formset.as_deref())
            && !src.formset_guid.eq_ignore_ascii_case(actual)
        {
            warn = format!(
                "warn: foreign formset (package {} → target {actual}) · ",
                src.formset_guid
            );
        }
    }

    let mut body: Option<String> = None;
    let mut inserted: Vec<u32> = Vec::new();
    let mut string_ids = std::collections::HashMap::new();
    if !env.body.is_empty() {
        let stores = client
            .inner
            .hii_list_varstores(auth_req(
                &client.state,
                HiiListVarstoresRequest {
                    image_id: image_id.into(),
                    target: target.into(),
                },
            ))
            .await
            .map_err(|e| e.message().to_string())?
            .into_inner()
            .varstores;
        let planned = uefi_common::envelope::plan_varstores(&env.body, &varstore_briefs(&stores)?)
            .map_err(|e| e.to_string())?;
        let r = client
            .inner
            .hii_form_add(auth_req(
                &client.state,
                HiiFormAddRequest {
                    image_id: image_id.into(),
                    target: target.into(),
                    schema_json: planned.clone(),
                },
            ))
            .await
            .map_err(|e| e.message().to_string())?
            .into_inner();
        inserted = r.inserted_form_ids;
        string_ids = r.string_ids;
        body = Some(planned);
    }

    let ref_outcome = match env.refs.as_ref() {
        None => None,
        Some(refs) => Some(
            match uefi_common::envelope::plan_ref_step(refs, body.as_deref(), &inserted, &busy) {
                Ok(Some(plan)) => {
                    let schema =
                        serde_json::to_string(&serde_json::json!({ "refs": plan.records }))
                            .map_err(|e| format!("refs plan: {e}"))?;
                    let parent = format!("{}#{}", target, refs.parent_form_id);
                    let qids: Vec<u16> = plan.records.iter().map(|r| r.question_id).collect();
                    client
                        .inner
                        .hii_question_add(auth_req(
                            &client.state,
                            HiiQuestionAddRequest {
                                image_id: image_id.into(),
                                target: parent,
                                schema_json: schema,
                            },
                        ))
                        .await
                        .map(|_| (refs.parent_form_id, qids))
                        .map_err(|e| e.message().to_string())
                }
                Ok(None) => Err("refs plan is empty".to_string()),
                Err(e) => Err(e.to_string()),
            },
        ),
    };

    refresh_tree(app, client).await?;
    reload_forms(app, client).await?;
    let _ = refresh_form_details_if_needed(app, client).await;
    app.status_msg = format!(
        "{warn}{}",
        import_status(&inserted, &string_ids, ref_outcome.as_ref())
    );
    Ok(if inserted.is_empty() {
        target.to_string()
    } else {
        inserted
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",")
    })
}

pub async fn execute_command(
    app: &mut App,
    cmdline: &str,
    client: &mut Client,
) -> Result<String, String> {
    let parts: Vec<&str> = cmdline.split_whitespace().collect();
    if parts.is_empty() {
        return Err("empty command".into());
    }
    let cmd = parts[0];
    match cmd {
        "open" | "o" => {
            let path = parts
                .get(1)
                .ok_or("usage: :open PATH [--mode read|write]")?;
            let mode = if parts.contains(&"write") { 1 } else { 0 };
            let sid = client.state.session_id.clone().ok_or("no session")?;
            let req = ImageOpenRequest {
                session_id: sid,
                path: path.to_string(),
                mode,
                name: String::new(),
            };
            let r = client
                .inner
                .image_open(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.image_loaded = true;
            app.status_msg = format!("opened image {}", r.image_id);
            let dump = client
                .inner
                .image_nodes_list(auth_req(
                    &client.state,
                    ImageNodesListRequest {
                        image_id: r.image_id.clone(),
                        filter: String::new(),
                    },
                ))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.tree = crate::tree::build_tree(&dump.nodes);
            app.cursor = 0;
            app.active_image_id = Some(r.image_id.clone());
            client.state.active_image_id = Some(r.image_id.clone());
            let _ = refresh_registry(app, client).await;
            Ok(r.image_id)
        }
        "upload" => {
            let path = parts
                .get(1)
                .ok_or("usage: :upload PATH [--mode read|write]")?;
            let mode = if parts.contains(&"write") { 1 } else { 0 };
            let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            let name = std::path::Path::new(path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let sid = client.state.session_id.clone().ok_or("no session")?;
            let resp = client.image_upload(&sid, bytes, mode, &name).await?;
            app.active_image_id = Some(resp.image_id.clone());
            client.state.active_image_id = Some(resp.image_id.clone());
            app.image_loaded = true;
            app.cursor = 0;
            refresh_tree(app, client).await?;
            refresh_registry(app, client).await?;
            app.status_msg = format!("uploaded {} ({})", resp.name, resp.image_id);
            Ok(resp.image_id)
        }
        "snapshot" => {
            let iid = client
                .state
                .active_image_id
                .clone()
                .ok_or("no active image")?;
            let name = parts.get(1).copied().unwrap_or_default().to_string();
            let resp = client.image_snapshot_create(&iid, &name).await?;
            app.status_msg = format!("snapshot {} ({})", resp.snapshot_id, name);
            Ok(resp.snapshot_id)
        }
        "snapshots" => {
            let iid = client
                .state
                .active_image_id
                .clone()
                .ok_or("no active image")?;
            let snaps = client.image_snapshots_list(&iid).await?;
            app.status_msg = if snaps.is_empty() {
                "no snapshots".into()
            } else {
                snaps
                    .iter()
                    .map(|s| format!("{}  {}  {}B", s.snapshot_id, s.name, s.size))
                    .collect::<Vec<_>>()
                    .join("  ·  ")
            };
            Ok(snaps
                .iter()
                .map(|s| s.snapshot_id.clone())
                .collect::<Vec<_>>()
                .join(","))
        }
        "restore" => {
            let iid = client
                .state
                .active_image_id
                .clone()
                .ok_or("no active image")?;
            let snap_id = parts
                .get(1)
                .ok_or("usage: :restore SNAPSHOT_ID (see :snapshots)")?;
            client.image_snapshot_restore(&iid, snap_id).await?;
            refresh_tree(app, client).await?;
            refresh_registry(app, client).await?;
            app.status_msg = format!("restored {snap_id}");
            Ok(snap_id.to_string())
        }
        "save" | "s" => {
            let raw = parts.get(1).ok_or("usage: :save OUTPUT")?;
            let path = absolutize_path(raw);
            let iid = client
                .state
                .active_image_id
                .clone()
                .ok_or("no active image")?;
            let req = ImageSaveRequest {
                image_id: iid,
                output_path: path.to_string(),
            };
            client
                .inner
                .image_save(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?;
            app.status_msg = format!("saved to {path}");
            Ok(path.to_string())
        }
        "extract" => {
            let target = parts.get(1).ok_or("usage: :extract TARGET [--body-only]")?;
            let body_only = parts.contains(&"--body-only");
            let iid = client
                .state
                .active_image_id
                .clone()
                .ok_or("no active image")?;
            let req = ImageNodeExtractRequest {
                image_id: iid,
                target: target.to_string(),
                body_only,
            };
            let r = client
                .inner
                .image_node_extract(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.status_msg = format!("extracted artifact {}", r.artifact_id);
            let _ = refresh_registry(app, client).await;
            Ok(r.artifact_id)
        }
        "export" => {
            let artifact_id = parts.get(1).ok_or("usage: :export ARTIFACT_ID [PATH]")?;
            let path = export_output_path(parts.get(2).copied(), artifact_id);
            let req = ArtifactExportRequest {
                artifact_id: artifact_id.to_string(),
                output_path: path.clone(),
            };
            client
                .inner
                .artifact_export(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?;
            app.status_msg = format!("exported {artifact_id} to {path}");
            Ok(artifact_id.to_string())
        }
        "import" => {
            let file = parts.get(1).ok_or("usage: :import FILE")?;
            let sid = client.state.session_id.clone().ok_or("no session")?;
            let req = ArtifactImportRequest {
                session_id: sid,
                path: file.to_string(),
            };
            let r = client
                .inner
                .artifact_import(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.status_msg = format!("imported artifact {}", r.artifact_id);
            let _ = refresh_registry(app, client).await;
            Ok(r.artifact_id)
        }
        "artifacts" => {
            let sid = client.state.session_id.clone().ok_or("no session")?;
            let req = ArtifactsListRequest { session_id: sid };
            let r = client
                .inner
                .artifacts_list(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.status_msg = format!("{} artifacts", r.artifacts.len());
            Ok(format!("{} artifacts", r.artifacts.len()))
        }
        "insert" => {
            let a = parse_node_flags(&parts);
            let iid = client
                .state
                .active_image_id
                .clone()
                .ok_or("no active image")?;
            let target = a
                .target
                .clone()
                .or_else(|| app.selected_path())
                .ok_or("no target (select a node or pass TARGET)")?;
            let (ffs_path, artifact_id) = match a.source() {
                Ok(Some(Source::File)) => (a.file.unwrap_or_default(), String::new()),
                Ok(Some(Source::Artifact)) => (String::new(), a.artifact_id.unwrap_or_default()),
                Ok(None) => return Err("exactly one of --file / --artifact-id is required".into()),
                Err(e) => return Err(e),
            };
            let mode = mode_to_i32(a.mode.as_deref().unwrap_or(""))?;
            let req = ImageNodeInsertRequest {
                image_id: iid,
                target,
                ffs_path,
                artifact_id,
                mode,
            };
            let r = client
                .inner
                .image_node_insert(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.status_msg = format!("inserted {}", r.item_id);
            let _ = refresh_registry(app, client).await;
            let _ = refresh_tree(app, client).await;
            Ok(r.item_id)
        }
        "replace" => {
            let a = parse_node_flags(&parts);
            let iid = client
                .state
                .active_image_id
                .clone()
                .ok_or("no active image")?;
            let target = a
                .target
                .clone()
                .or_else(|| app.selected_path())
                .ok_or("no target")?;
            let (ffs_path, artifact_id) = match a.source() {
                Ok(Some(Source::File)) => (a.file.unwrap_or_default(), String::new()),
                Ok(Some(Source::Artifact)) => (String::new(), a.artifact_id.unwrap_or_default()),
                Ok(None) => return Err("exactly one of --file / --artifact-id is required".into()),
                Err(e) => return Err(e),
            };
            let req = ImageNodeReplaceRequest {
                image_id: iid,
                target,
                ffs_path,
                artifact_id,
                body_only: a.body_only,
            };
            let r = client
                .inner
                .image_node_replace(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.status_msg = format!("replaced {}", r.item_id);
            let _ = refresh_registry(app, client).await;
            let _ = refresh_tree(app, client).await;
            Ok(r.item_id)
        }
        "remove" => {
            let a = parse_node_flags(&parts);
            let iid = client
                .state
                .active_image_id
                .clone()
                .ok_or("no active image")?;
            let target = a
                .target
                .or_else(|| app.selected_path())
                .ok_or("no target")?;
            let req = ImageNodeRemoveRequest {
                image_id: iid,
                target: target.clone(),
            };
            client
                .inner
                .image_node_remove(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?;
            app.status_msg = format!("removed {target}");
            let _ = refresh_tree(app, client).await;
            Ok(target)
        }
        "rebuild" => {
            let a = parse_node_flags(&parts);
            let iid = client
                .state
                .active_image_id
                .clone()
                .ok_or("no active image")?;
            let target = a
                .target
                .or_else(|| app.selected_path())
                .ok_or("no target")?;
            let req = ImageNodeRebuildRequest {
                image_id: iid,
                target: target.clone(),
            };
            client
                .inner
                .image_node_rebuild(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?;
            app.status_msg = format!("rebuilt {target}");
            let _ = refresh_tree(app, client).await;
            Ok(target)
        }
        "image" => {
            if parts.len() > 1 {
                return Err("image subcommands moved: :switch ID | :close [ID]".into());
            }
            app.view = View::Image;
            Ok("image".into())
        }
        "switch" => {
            let id = parts.get(1).ok_or("usage: :switch ID")?.to_string();
            let dump = client
                .inner
                .image_nodes_list(auth_req(
                    &client.state,
                    ImageNodesListRequest {
                        image_id: id.clone(),
                        filter: String::new(),
                    },
                ))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            app.tree = crate::tree::build_tree(&dump.nodes);
            app.cursor = 0;
            app.active_image_id = Some(id.clone());
            client.state.active_image_id = Some(id.clone());
            app.image_loaded = true;
            app.status_msg = format!("switched to {id}");
            let _ = refresh_registry(app, client).await;
            if app.view == View::Forms {
                refresh_forms(app, client).await?;
            }
            Ok(id)
        }
        "close" => {
            let id = parts
                .get(1)
                .map(|s| s.to_string())
                .or_else(|| client.state.active_image_id.clone())
                .ok_or("no active image")?;
            let req = ImageCloseRequest {
                image_id: id.clone(),
            };
            client
                .inner
                .image_close(auth_req(&client.state, req))
                .await
                .map_err(|e| e.message().to_string())?
                .into_inner();
            if app.active_image_id.as_deref() == Some(id.as_str()) {
                app.active_image_id = None;
                client.state.active_image_id = None;
                app.tree.clear();
                app.cursor = 0;
                app.image_loaded = false;
                app.view = View::Image;
            }
            app.status_msg = format!("closed {id}");
            let _ = refresh_registry(app, client).await;
            Ok(id)
        }
        "reopen" => {
            let want_write = !parts.contains(&"read");
            reopen(app, client, want_write).await
        }
        "forms" | "f" => {
            if app.active_image_id.is_none() && client.state.active_image_id.is_none() {
                return Err("no active image — :open PATH first".into());
            }
            refresh_forms(app, client).await?;
            app.view = View::Forms;
            app.status_msg = format!("forms: {}", app.forms.forms.len());
            Ok("forms".into())
        }
        "hii" => {
            let sub = parts.get(1).copied().ok_or(
                "usage: :hii set-value ITEM VALUE | :hii visibility ITEM on|off | :hii unlock ITEM | :hii formset add FILE [--ffs GUID] | :hii form add TARGET FILE | :hii form export ITEM [--out FILE] | :hii question add TARGET#FORM FILE | :hii page add TARGET FILE | :hii hijack TARGET FILE [SETUPDATA-GUID] | :hii import TARGET --file FILE",
            )?;
            let iid = client
                .state
                .active_image_id
                .clone()
                .ok_or("no active image")?;
            match sub {
                "set-value" => {
                    let item = parts
                        .get(2)
                        .ok_or("usage: :hii set-value ITEM VALUE")?
                        .to_string();
                    let value =
                        parse_u64_loose(parts.get(3).ok_or("usage: :hii set-value ITEM VALUE")?)?;
                    let r = client
                        .inner
                        .hii_set_value(auth_req(
                            &client.state,
                            HiiSetValueRequest {
                                image_id: iid,
                                item_id: item.clone(),
                                value,
                            },
                        ))
                        .await
                        .map_err(|e| e.message().to_string())?
                        .into_inner();
                    let prev_qid = app.selected_question_id();
                    reload_forms(app, client).await?;
                    let _ = refresh_form_details_if_needed(app, client).await;
                    if let Some(qid) = prev_qid
                        && let Some(pos) = app
                            .forms
                            .questions
                            .iter()
                            .position(|q| q.question_id == qid)
                    {
                        app.forms.question_cursor = pos;
                        let _ = refresh_question_info_if_needed(app, client).await;
                    }
                    let flips = if r.applied_flips.is_empty() {
                        "none".to_string()
                    } else {
                        r.applied_flips.join(" · ")
                    };
                    let stores = if r.stores.is_empty() {
                        "none".to_string()
                    } else {
                        r.stores.join(" · ")
                    };
                    app.status_msg = format!("set {item}={value}: flips {flips} · stores {stores}");
                    Ok(item)
                }
                "visibility" => {
                    let item = parts
                        .get(2)
                        .ok_or("usage: :hii visibility ITEM on|off")?
                        .to_string();
                    let visible = match parts.get(3) {
                        Some(&"on") => true,
                        Some(&"off") => false,
                        _ => return Err("usage: :hii visibility ITEM on|off".into()),
                    };
                    client
                        .inner
                        .hii_set_form_visibility(auth_req(
                            &client.state,
                            HiiSetFormVisibilityRequest {
                                image_id: iid,
                                item_id: item.clone(),
                                visible,
                            },
                        ))
                        .await
                        .map_err(|e| e.message().to_string())?;
                    reload_forms(app, client).await?;
                    let _ = refresh_form_details_if_needed(app, client).await;
                    app.status_msg =
                        format!("visibility {item}: {}", if visible { "on" } else { "off" });
                    Ok(item)
                }
                "unlock" => {
                    let item = parts.get(2).ok_or("usage: :hii unlock ITEM")?.to_string();
                    let r = client
                        .inner
                        .hii_unlock(auth_req(
                            &client.state,
                            HiiUnlockRequest {
                                image_id: iid,
                                item_id: item.clone(),
                            },
                        ))
                        .await
                        .map_err(|e| e.message().to_string())?
                        .into_inner();
                    reload_forms(app, client).await?;
                    let _ = refresh_form_details_if_needed(app, client).await;
                    app.status_msg = if r.applied_flips.is_empty() {
                        format!(
                            "unlock {item}: no flippable gates ({} gates)",
                            r.gates.len()
                        )
                    } else {
                        format!("unlock {item}: {}", r.applied_flips.join(" · "))
                    };
                    Ok(item)
                }
                "formset" => {
                    if parts.get(2).copied() != Some("add") {
                        return Err("usage: :hii formset add FILE [--ffs GUID]".into());
                    }
                    let file = parts
                        .get(3)
                        .ok_or("usage: :hii formset add FILE [--ffs GUID]")?;
                    let ffs = parts
                        .iter()
                        .position(|p| *p == "--ffs")
                        .and_then(|i| parts.get(i + 1).copied())
                        .unwrap_or_default();
                    let schema_json = read_schema(file)?;
                    let r = client
                        .inner
                        .hii_form_set_add(auth_req(
                            &client.state,
                            HiiFormSetAddRequest {
                                image_id: iid,
                                schema_json,
                                target_ffs_guid: ffs.to_string(),
                            },
                        ))
                        .await
                        .map_err(|e| e.message().to_string())?
                        .into_inner();
                    refresh_tree(app, client).await?;
                    reload_forms(app, client).await?;
                    let _ = refresh_form_details_if_needed(app, client).await;
                    app.status_msg =
                        formset_add_status(&r.new_ffs_id, &r.inserted_form_ids, &r.string_ids);
                    Ok(r.new_ffs_id)
                }
                "form" => match parts.get(2).copied() {
                    Some("add") => {
                        let target = parts.get(3).ok_or("usage: :hii form add TARGET FILE")?;
                        let file = parts.get(4).ok_or("usage: :hii form add TARGET FILE")?;
                        let schema_json = read_schema(file)?;
                        ensure_bare_schema(&schema_json)?;
                        let r = client
                            .inner
                            .hii_form_add(auth_req(
                                &client.state,
                                HiiFormAddRequest {
                                    image_id: iid,
                                    target: target.to_string(),
                                    schema_json,
                                },
                            ))
                            .await
                            .map_err(|e| e.message().to_string())?
                            .into_inner();
                        refresh_tree(app, client).await?;
                        reload_forms(app, client).await?;
                        let _ = refresh_form_details_if_needed(app, client).await;
                        app.status_msg = form_add_status(&r.inserted_form_ids, &r.string_ids);
                        Ok(r.inserted_form_ids
                            .iter()
                            .map(|i| i.to_string())
                            .collect::<Vec<_>>()
                            .join(","))
                    }
                    Some("export") => {
                        let item = parts
                            .get(3)
                            .ok_or("usage: :hii form export ITEM [--out FILE]")?
                            .to_string();
                        let out = parts
                            .iter()
                            .position(|p| *p == "--out")
                            .and_then(|i| parts.get(i + 1).copied());
                        hii_form_export(app, client, &iid, &item, out).await
                    }
                    _ => Err(
                        "usage: :hii form add TARGET FILE | :hii form export ITEM [--out FILE]"
                            .into(),
                    ),
                },
                "import" => {
                    let target = parts
                        .get(2)
                        .ok_or("usage: :hii import TARGET --file FILE")?
                        .to_string();
                    let file = parts
                        .iter()
                        .position(|p| *p == "--file")
                        .and_then(|i| parts.get(i + 1).copied())
                        .ok_or("usage: :hii import TARGET --file FILE")?
                        .to_string();
                    hii_import(app, client, &iid, &target, &file).await
                }
                "question" => {
                    if parts.get(2).copied() != Some("add") {
                        return Err(
                            "usage: :hii question add TARGET#FORM FILE (insert_before {goto_form_id|question_id} в JSON задаёт позицию; без — конец формы)".into(),
                        );
                    }
                    let item = parts
                        .get(3)
                        .ok_or("usage: :hii question add TARGET#FORM FILE (insert_before {goto_form_id|question_id} в JSON задаёт позицию; без — конец формы)")?;
                    let file = parts
                        .get(4)
                        .ok_or("usage: :hii question add TARGET#FORM FILE (insert_before {goto_form_id|question_id} в JSON задаёт позицию; без — конец формы)")?;
                    let schema_json = read_schema(file)?;
                    let r = client
                        .inner
                        .hii_question_add(auth_req(
                            &client.state,
                            HiiQuestionAddRequest {
                                image_id: iid,
                                target: item.to_string(),
                                schema_json,
                            },
                        ))
                        .await
                        .map_err(|e| e.message().to_string())?
                        .into_inner();
                    refresh_tree(app, client).await?;
                    reload_forms(app, client).await?;
                    let _ = refresh_form_details_if_needed(app, client).await;
                    app.status_msg = question_add_status(&r.questions, &r.refs);
                    Ok(r.questions
                        .iter()
                        .map(|o| format!("{:#x}", o.question_id))
                        .collect::<Vec<_>>()
                        .join(","))
                }
                "page" => {
                    if parts.get(2).copied() != Some("add") {
                        return Err("usage: :hii page add TARGET FILE".into());
                    }
                    let target = parts.get(3).ok_or("usage: :hii page add TARGET FILE")?;
                    let file = parts.get(4).ok_or("usage: :hii page add TARGET FILE")?;
                    let schema_json = read_schema(file)?;
                    let r = client
                        .inner
                        .hii_page_add(auth_req(
                            &client.state,
                            HiiPageAddRequest {
                                image_id: iid,
                                target: target.to_string(),
                                schema_json,
                            },
                        ))
                        .await
                        .map_err(|e| e.message().to_string())?
                        .into_inner();
                    refresh_tree(app, client).await?;
                    reload_forms(app, client).await?;
                    let _ = refresh_form_details_if_needed(app, client).await;
                    app.status_msg = page_add_status(&r);
                    Ok(r.form_id.to_string())
                }
                "hijack" => {
                    let target = parts
                        .get(2)
                        .ok_or("usage: :hii hijack TARGET FILE [SETUPDATA-GUID]")?;
                    let file = parts
                        .get(3)
                        .ok_or("usage: :hii hijack TARGET FILE [SETUPDATA-GUID]")?;
                    let setupdata_guid = parts.get(4).copied().unwrap_or_default();
                    let schema_json = read_schema(file)?;
                    let r = client
                        .inner
                        .hii_form_hijack(auth_req(
                            &client.state,
                            HiiFormHijackRequest {
                                image_id: iid,
                                target: target.to_string(),
                                schema_json,
                                setupdata_guid: setupdata_guid.to_string(),
                            },
                        ))
                        .await
                        .map_err(|e| e.message().to_string())?
                        .into_inner();
                    refresh_tree(app, client).await?;
                    reload_forms(app, client).await?;
                    let _ = refresh_form_details_if_needed(app, client).await;
                    app.status_msg = hijack_status(&r);
                    Ok(target.to_string())
                }
                other => Err(format!("unknown hii subcommand: {other}")),
            }
        }
        "nvar" => {
            let sub = parts.get(1).copied().ok_or(NVAR_USAGE)?;
            let iid = client
                .state
                .active_image_id
                .clone()
                .ok_or("no active image")?;
            match sub {
                "list" => {
                    let path = parts.get(2).filter(|p| !p.starts_with("--")).copied();
                    let var = flag_value(&parts, "--var");
                    if path.is_some() && var.is_some() {
                        return Err(
                            "usage: :nvar list [PATH] [--var NAME] — --var only without PATH"
                                .into(),
                        );
                    }
                    match path {
                        Some(p) => {
                            app.goto_path(p)?;
                            let resp = client
                                .inner
                                .nvar_list(auth_req(
                                    &client.state,
                                    NvarListRequest {
                                        image_id: iid.clone(),
                                        path: Some(p.to_string()),
                                        include_data: true,
                                    },
                                ))
                                .await
                                .map_err(|e| e.message().to_string())?
                                .into_inner();
                            let store = resp
                                .stores
                                .first()
                                .ok_or_else(|| format!("no NVRAM store at {p}"))?;
                            let vars = store.vars.len();
                            let records = store.records;
                            let free_tail = store.free_tail;
                            app.nvar.stores = resp.stores;
                            app.nvar.cursor = 0;
                            app.nvar.hex_scroll = 0;
                            app.nvar.list_state = ratatui::widgets::ListState::default();
                            app.nvar.key = Some(format!("{iid}:{p}"));
                            app.status_msg = format!(
                                "nvar {p}: {vars} vars · records {records} · free {free_tail:#x}B"
                            );
                            Ok(p.to_string())
                        }
                        None => {
                            let resp = client
                                .inner
                                .nvar_list(auth_req(
                                    &client.state,
                                    NvarListRequest {
                                        image_id: iid,
                                        path: None,
                                        include_data: false,
                                    },
                                ))
                                .await
                                .map_err(|e| e.message().to_string())?
                                .into_inner();
                            match var {
                                Some(name) => {
                                    let rows = nvar_var_rows(&resp.stores, name);
                                    app.status_msg = if rows.is_empty() {
                                        format!("nvar: no vars named {name}")
                                    } else {
                                        format!(
                                            "nvar {name}: {} · {}",
                                            rows.len(),
                                            rows.join(" · ")
                                        )
                                    };
                                    Ok(name.to_string())
                                }
                                None => {
                                    app.status_msg = nvar_summary_status(&resp.stores);
                                    Ok(resp.stores.len().to_string())
                                }
                            }
                        }
                    }
                }
                "set" => {
                    let args = parse_nvar_set(&parts)?;
                    let resp = client
                        .inner
                        .nvar_set(auth_req(
                            &client.state,
                            NvarSetRequest {
                                image_id: iid,
                                name: args.name.clone(),
                                guid: args.guid.clone(),
                                offset: args.offset,
                                value: args.value,
                                width: args.width,
                            },
                        ))
                        .await
                        .map_err(|e| e.message().to_string())?
                        .into_inner();
                    app.nvar.key = None;
                    if app.selected_is_nvar() {
                        let _ = refresh_nvars_if_needed(app, client).await;
                    }
                    let applied = if resp.applied.is_empty() {
                        "none".to_string()
                    } else {
                        resp.applied.join(" · ")
                    };
                    app.status_msg = format!(
                        "nvar set {}+{:#x}={:#x} (w{}): applied {} · stores {}",
                        args.name,
                        args.offset,
                        args.value,
                        args.width,
                        applied,
                        resp.stores.len()
                    );
                    Ok(args.name)
                }
                _ => Err(NVAR_USAGE.into()),
            }
        }
        "filter" => {
            if !app.forms.show_strings {
                return Err("filter is for the strings browser (S to open)".into());
            }
            app.forms.strings_filter = parts[1..].join(" ");
            app.forms.strings_cursor = 0;
            Ok(app.forms.strings_filter.clone())
        }
        "refresh" => {
            refresh_registry(app, client).await?;
            app.status_msg = format!(
                "registry: {} images, {} artifacts",
                app.registry.images.len(),
                app.registry.artifacts.len()
            );
            Ok("refreshed".into())
        }
        "goto" | "g" => {
            let target = parts.get(1).ok_or("usage: :goto PATH (e.g. 1/28/1)")?;
            app.goto_path(target)?;
            Ok(format!("→ {target}"))
        }
        "quit" | "q" => {
            app.quit = true;
            Ok("quitting".into())
        }
        "help" | "h" => {
            app.toggle_help();
            Ok("help toggled".into())
        }
        _ => Err(format!("unknown command: :{cmd}, try :help")),
    }
}

const COMMANDS: &[&str] = &[
    "open",
    "o",
    "save",
    "s",
    "extract",
    "export",
    "import",
    "artifacts",
    "insert",
    "replace",
    "remove",
    "rebuild",
    "switch",
    "close",
    "reopen",
    "image",
    "refresh",
    "forms",
    "f",
    "filter",
    "goto",
    "g",
    "hii",
    "nvar",
    "upload",
    "snapshot",
    "snapshots",
    "restore",
    "help",
    "h",
    "quit",
    "q",
];

pub struct Completion {
    pub common: Option<String>,
    pub items: Vec<crate::app::MenuItem>,
}

pub fn complete(app: &App, cmdline: &str) -> Completion {
    let ends_space = cmdline.ends_with(' ');
    let mut parts: Vec<&str> = cmdline.split_whitespace().collect();
    let token = if ends_space || parts.is_empty() {
        String::new()
    } else {
        parts.pop().unwrap().to_string()
    };
    let head: Vec<&str> = parts.clone();
    let candidates: Vec<String> = if head.is_empty() {
        COMMANDS
            .iter()
            .filter(|c| c.starts_with(token.as_str()))
            .map(|s| s.to_string())
            .collect()
    } else {
        context_candidates(app, head[0], &head, &token)
    };
    if candidates.is_empty() {
        return Completion {
            common: None,
            items: vec![],
        };
    }
    let mut base = head.join(" ");
    if !base.is_empty() {
        base.push(' ');
    }
    let items: Vec<crate::app::MenuItem> = candidates
        .iter()
        .map(|c| {
            let mut apply = base.clone();
            apply.push_str(c);
            if !c.ends_with('/') {
                apply.push(' ');
            }
            crate::app::MenuItem {
                display: c.clone(),
                apply,
            }
        })
        .collect();
    if candidates.len() == 1 {
        return Completion {
            common: Some(items[0].apply.clone()),
            items: vec![],
        };
    }
    let prefix = common_prefix(&candidates);
    base.push_str(&prefix);
    Completion {
        common: Some(base),
        items,
    }
}

fn common_prefix(items: &[String]) -> String {
    let mut p = items[0].clone();
    for s in items {
        p.truncate(p.chars().zip(s.chars()).take_while(|(a, b)| a == b).count());
    }
    p
}

fn fmt_item(target: impl std::fmt::Display, form_id_ifr: u32) -> String {
    format!("{}#{}", target, form_id_ifr)
}

fn unique_form_targets(app: &App) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    app.forms
        .forms
        .iter()
        .map(|f| f.form_id.clone())
        .filter(|t| seen.insert(t.clone()))
        .collect()
}

fn unique_formset_guids(app: &App) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    app.forms
        .forms
        .iter()
        .map(|f| f.formset_guid.clone())
        .filter(|g| seen.insert(g.clone()))
        .collect()
}

/// Уникальные имена переменных всех загруженных сторов NVAR-панель
/// (для `:nvar set NAME` и `--var`).
fn nvar_var_names(app: &App) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    app.nvar
        .stores
        .iter()
        .flat_map(|s| s.vars.iter())
        .map(|v| v.name.clone())
        .filter(|n| seen.insert(n.clone()))
        .collect()
}

/// item_id-кандидаты (`<target>#<form_id>`, form_id десятичное — контракт
/// parse_item_id): общий список для question add / set-value /
/// visibility / unlock / form export. Спека hii-form-export §4.
fn item_id_candidates(app: &App) -> Vec<String> {
    app.forms
        .forms
        .iter()
        .map(|f| fmt_item(&f.form_id, f.form_id_ifr))
        .collect()
}

/// Пути по префиксу для позиции schema-файла: набранный каталог-префикс
/// сохраняется как есть (абсолютный/относительный), каталоги получают
/// "/", скрытые файлы — только по точечному префиксу. Спека §4 V3.
fn expand_tilde(token: &str, home: &str) -> String {
    if token == "~" {
        return home.to_string();
    }
    if let Some(rest) = token.strip_prefix("~/") {
        return format!("{home}/{rest}");
    }
    token.to_string()
}

fn complete_path(token: &str) -> Vec<String> {
    let expanded = expand_tilde(token, &std::env::var("HOME").unwrap_or_default());
    let token = expanded.as_str();
    let (dir_part, prefix) = match token.rfind('/') {
        Some(i) => (&token[..=i], &token[i + 1..]),
        None => ("", token),
    };
    let dir = if dir_part.is_empty() { "." } else { dir_part };
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut out: Vec<String> = rd
        .flatten()
        .map(|e| {
            (
                e.file_name().to_string_lossy().to_string(),
                std::fs::metadata(e.path())
                    .map(|m| m.is_dir())
                    .unwrap_or(false),
            )
        })
        .filter(|(name, _)| name.starts_with(prefix))
        .filter(|(name, _)| !name.starts_with('.') || prefix.starts_with('.'))
        .map(|(name, is_dir)| format!("{dir_part}{name}{}", if is_dir { "/" } else { "" }))
        .collect();
    out.sort();
    out
}

fn context_candidates(app: &App, cmd: &str, head: &[&str], token: &str) -> Vec<String> {
    if head.last() == Some(&"--file") {
        return complete_path(token);
    }
    if cmd == "hii"
        && head.get(1) == Some(&"form")
        && head.get(2) == Some(&"export")
        && head.last() == Some(&"--out")
    {
        return complete_path(token);
    }
    if head.last() == Some(&"--mode") {
        let vals: &[&str] = if matches!(cmd, "reopen" | "open" | "o") {
            &["read", "write"]
        } else {
            &["into", "before", "after"]
        };
        return vals
            .iter()
            .filter(|c| c.starts_with(token))
            .map(|s| s.to_string())
            .collect();
    }
    if head.last() == Some(&"--artifact-id") {
        return app
            .registry
            .artifacts
            .iter()
            .map(|a| a.artifact_id.clone())
            .filter(|c| c.starts_with(token))
            .collect();
    }
    if matches!(cmd, "switch" | "close") && head.len() == 1 {
        return app
            .registry
            .images
            .iter()
            .map(|im| im.image_id.clone())
            .filter(|c| c.starts_with(token))
            .collect();
    }
    if head.last() == Some(&"--ffs") {
        return unique_formset_guids(app)
            .into_iter()
            .filter(|c| c.starts_with(token))
            .collect();
    }
    if head.last() == Some(&"--var") {
        return nvar_var_names(app)
            .into_iter()
            .filter(|c| c.starts_with(token))
            .collect();
    }
    if cmd == "nvar" && head.last() == Some(&"--guid") {
        let mut seen = std::collections::HashSet::new();
        return app
            .nvar
            .stores
            .iter()
            .flat_map(|s| s.vars.iter())
            .filter(|v| !v.guid.is_empty())
            .map(|v| v.guid.clone())
            .filter(|g| seen.insert(g.clone()))
            .filter(|g| g.starts_with(token))
            .collect();
    }
    if cmd == "nvar" && head.last() == Some(&"--width") {
        return ["1", "2", "4", "8"]
            .iter()
            .filter(|c| c.starts_with(token))
            .map(|s| s.to_string())
            .collect();
    }
    if token.starts_with("--") {
        let flags: &[&str] = match cmd {
            "insert" => &["--file", "--artifact-id", "--mode"],
            "replace" => &["--file", "--artifact-id", "--body-only"],
            "extract" => &["--body-only"],
            "reopen" => &["--mode"],
            "open" | "o" => &["--mode"],
            "hii" if head.len() == 4 && head[1] == "formset" && head[2] == "add" => &["--ffs"],
            "hii" if head.len() == 4 && head[1] == "form" && head[2] == "export" => &["--out"],
            "hii" if head.len() == 3 && head[1] == "import" => &["--file"],
            "nvar" if head.len() == 2 && head[1] == "list" => &["--var"],
            "nvar" if head.len() >= 2 && head[1] == "set" => {
                &["--guid", "--offset", "--value", "--width"]
            }
            _ => &[],
        };
        return flags
            .iter()
            .filter(|c| c.starts_with(token))
            .map(|s| s.to_string())
            .collect();
    }
    if cmd == "export" {
        if head.len() == 1 {
            return app
                .registry
                .artifacts
                .iter()
                .map(|a| a.artifact_id.clone())
                .filter(|c| c.starts_with(token))
                .collect();
        }
        if head.len() == 2 {
            return complete_path(token);
        }
        return vec![];
    }
    if matches!(cmd, "open" | "o" | "save" | "s" | "upload" | "import") && head.len() == 1 {
        return complete_path(token);
    }
    if cmd == "hii" {
        if head.len() == 1 {
            return [
                "formset",
                "form",
                "question",
                "page",
                "hijack",
                "set-value",
                "visibility",
                "unlock",
                "import",
            ]
            .iter()
            .filter(|c| c.starts_with(token))
            .map(|s| s.to_string())
            .collect();
        }
        let noun = head[1];
        if matches!(noun, "formset" | "question" | "page") {
            if head.len() == 2 {
                return ["add"]
                    .iter()
                    .filter(|c| c.starts_with(token))
                    .map(|s| s.to_string())
                    .collect();
            }
            if head.get(2) != Some(&"add") {
                return vec![];
            }
            let file_pos = if noun == "formset" { 3 } else { 4 };
            if head.len() == file_pos {
                return complete_path(token);
            }
            if head.len() == file_pos - 1 {
                if noun == "question" {
                    return item_id_candidates(app)
                        .into_iter()
                        .filter(|c| c.starts_with(token))
                        .collect();
                }
                return unique_form_targets(app)
                    .into_iter()
                    .filter(|c| c.starts_with(token))
                    .collect();
            }
            return vec![];
        }
        if noun == "form" {
            if head.len() == 2 {
                return ["add", "export"]
                    .iter()
                    .filter(|c| c.starts_with(token))
                    .map(|s| s.to_string())
                    .collect();
            }
            match head.get(2) {
                Some(&"add") if head.len() == 4 => {
                    return complete_path(token);
                }
                Some(&"add") if head.len() == 3 => {
                    return unique_form_targets(app)
                        .into_iter()
                        .filter(|c| c.starts_with(token))
                        .collect();
                }
                Some(&"export") if head.len() == 3 => {
                    return item_id_candidates(app)
                        .into_iter()
                        .filter(|c| c.starts_with(token))
                        .collect();
                }
                _ => {}
            }
            return vec![];
        }
        if noun == "import" {
            if head.len() == 2 {
                return unique_form_targets(app)
                    .into_iter()
                    .filter(|c| c.starts_with(token))
                    .collect();
            }
            return vec![];
        }
        if noun == "hijack" {
            if head.len() == 2 {
                return unique_form_targets(app)
                    .into_iter()
                    .filter(|c| c.starts_with(token))
                    .collect();
            }
            if head.len() == 3 {
                return complete_path(token);
            }
            return vec![];
        }
        if matches!(head[1], "set-value" | "visibility" | "unlock")
            && !head[2..].iter().any(|s| !s.starts_with("--"))
        {
            return item_id_candidates(app)
                .into_iter()
                .filter(|c| c.starts_with(token))
                .collect();
        }
        return vec![];
    }
    if cmd == "nvar" {
        if head.len() == 1 {
            return ["list", "set"]
                .iter()
                .filter(|c| c.starts_with(token))
                .map(|s| s.to_string())
                .collect();
        }
        if head.len() == 2 && head[1] == "list" {
            return app
                .visible()
                .iter()
                .map(|&i| app.tree[i].path.clone())
                .filter(|p| p.starts_with(token))
                .collect();
        }
        if head.len() == 2 && head[1] == "set" {
            return nvar_var_names(app)
                .into_iter()
                .filter(|c| c.starts_with(token))
                .collect();
        }
        return vec![];
    }
    let has_positional = head[1..].iter().any(|s| !s.starts_with("--"));
    let target_cmds = [
        "insert", "replace", "remove", "rebuild", "extract", "goto", "restore",
    ];
    if target_cmds.contains(&cmd) && !has_positional {
        return app
            .visible()
            .iter()
            .map(|&i| app.tree[i].path.clone())
            .filter(|p| p.starts_with(token))
            .collect();
    }
    vec![]
}

pub async fn refresh_tree(app: &mut App, client: &mut Client) -> Result<(), String> {
    let iid = client
        .state
        .active_image_id
        .clone()
        .ok_or("no active image")?;
    let expanded: std::collections::HashSet<String> = app
        .tree
        .iter()
        .filter(|n| n.expanded)
        .map(|n| n.path.clone())
        .collect();
    let cursor_path = app.selected_path();
    let dump = client
        .inner
        .image_nodes_list(auth_req(
            &client.state,
            ImageNodesListRequest {
                image_id: iid,
                filter: String::new(),
            },
        ))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    app.tree = crate::tree::build_tree(&dump.nodes);
    for n in &mut app.tree {
        if expanded.contains(&n.path) {
            n.expanded = true;
        }
    }
    match cursor_path
        .as_deref()
        .and_then(|p| app.tree.iter().position(|n| n.path == p))
    {
        Some(idx) => {
            let vis = app.visible();
            if let Some(vis_idx) = vis.iter().position(|&v| v == idx) {
                app.cursor = vis_idx;
            } else {
                app.sanitize_cursor();
            }
        }
        None => app.sanitize_cursor(),
    }
    Ok(())
}

pub async fn refresh_registry(app: &mut App, client: &mut Client) -> Result<(), String> {
    let sid = client.state.session_id.clone().ok_or("no session")?;
    let imgs = client
        .inner
        .images_list(auth_req(
            &client.state,
            ImagesListRequest {
                session_id: sid.clone(),
            },
        ))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    let arts = client
        .inner
        .artifacts_list(auth_req(
            &client.state,
            ArtifactsListRequest { session_id: sid },
        ))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    app.registry.images = imgs.images;
    app.registry.artifacts = arts.artifacts;
    if app.registry.cursor >= app.registry_selectable().len() {
        app.registry.cursor = 0;
    }
    Ok(())
}

/// item_id выделенной формы: "<target>#<form_id>" (form_id —
/// десятичное, контракт parse_item_id). None — если строка не форма.
pub fn selected_form_item_id(app: &App) -> Option<String> {
    let key = app.selected_form_key()?;
    Some(fmt_item(&key.target, key.form_id_ifr))
}

/// target-часть выделенной формы ("<ffs>:<type>:<idx>" без `#`), для
/// RPC без form_id (list_varstores). None — если строка не форма.
/// Спека varstore-contract §5.
pub fn selected_form_target(app: &App) -> Option<String> {
    if let Some(k) = app.selected_form_key() {
        return Some(k.target);
    }
    let rows = app.forms_rows();
    let Some(crate::forms::FormsRow::FormSet { guid, .. }) = rows.get(app.forms.cursor) else {
        return None;
    };
    app.forms
        .forms
        .iter()
        .find(|f| f.formset_guid.eq_ignore_ascii_case(guid))
        .map(|f| f.form_id.clone())
}

/// Insert-prefill для Enter на вопросе: вопрос из question_cursor.
/// item_id-контракт: form_id — ДЕСЯТИЧНОЕ (parse_item_id), qid — hex.
/// Спека tui-forms-view §4 V2, решение D6.
pub fn set_value_prefill(app: &App) -> Option<String> {
    let key = app.selected_form_key()?;
    let qid = app.selected_question_id()?;
    Some(format!(
        "hii set-value {}#{}:{:#x} ",
        key.target, key.form_id_ifr, qid
    ))
}

/// Insert-prefill для клавиши `a` в Forms-view: FormSet-строка —
/// `hii formset add ` (target не нужен), Form-строка — `hii form add
/// <target> ` из выделения (решение D6); DanglingRef — None.
/// Спека tui-forms-view §4 V3.
pub fn add_prefill(app: &App) -> Option<String> {
    match app.forms_rows().get(app.forms.cursor) {
        Some(crate::forms::FormsRow::FormSet { .. }) => Some("hii formset add ".into()),
        Some(crate::forms::FormsRow::Form { key, .. }) => {
            Some(format!("hii form add {} ", key.target))
        }
        _ => None,
    }
}

/// Insert-prefill для клавиши `e` на Form-строке: экспорт формы в конверт.
/// Спека hii-form-export §4.
pub fn export_prefill(app: &App) -> Option<String> {
    selected_form_item_id(app).map(|item| format!("hii form export {item} --out "))
}

/// Insert-prefill для клавиши `I`: target из FormSet-строки под курсором
/// (form_id первой формы формсета — грамматика `<ffs>:<type>:<idx>`), на
/// прочих строках — bare. Спека hii-form-export §4.
pub fn import_prefill(app: &App) -> String {
    let target = match app.forms_rows().get(app.forms.cursor) {
        Some(crate::forms::FormsRow::FormSet { guid, .. }) => app
            .forms
            .forms
            .iter()
            .find(|f| &f.formset_guid == guid)
            .map(|f| f.form_id.clone()),
        _ => None,
    };
    match target {
        Some(t) => format!("hii import {t} "),
        None => "hii import ".into(),
    }
}

/// Insert-prefill для клавиши `A` на Form-строке: второй шаг ручного
/// импорта — вопрос-REF под формой строки (form_id десятичное, контракт
/// parse_item_id). Спека hii-form-export §4.
pub fn question_add_prefill(app: &App) -> Option<String> {
    let key = app.selected_form_key()?;
    Some(format!(
        "hii question add {}#{} ",
        key.target, key.form_id_ifr
    ))
}

/// Insert-prefill для клавиши `R` на Form-строке: refs-only импорт ссылки
/// на форму под курсором. Import не принимает `#parent` (docs:fix 001ad0a):
/// родитель — `refs.parent_form_id` пакета, target — формсет строки,
/// автор правит. Спека hii-form-export §4.
pub fn ref_import_prefill(key: &crate::forms::FormKey) -> String {
    format!("hii import {} --file refs.json", key.target)
}

/// Подсказка статуса для `R`: значения из строки под курсором для entries
/// пакета (form_id + formset_guid). Спека hii-form-export §4.
pub fn ref_import_hint(key: &crate::forms::FormKey) -> String {
    format!(
        "refs.json entries: form_id {} · formset_guid {}",
        key.form_id_ifr, key.formset_guid
    )
}

/// Prefill i/r: registry-курсор на артефакте → --artifact-id, иначе --file.
/// Спека R4 (вариант A, TODO:808).
pub fn mutation_prefill(
    kind: &str,
    path: &str,
    row: Option<&crate::app::RegistryRow>,
    artifacts: &[uefi_proto::ArtifactInfo],
) -> String {
    let artifact = row.and_then(|r| match r {
        crate::app::RegistryRow::Artifact(i) => artifacts.get(*i),
        crate::app::RegistryRow::Image(_) => None,
    });
    match artifact {
        Some(a) => format!("{kind} {path} --artifact-id {} ", a.artifact_id),
        None => format!("{kind} {path} --file "),
    }
}

/// Гвард-решение :reopen по желаемому и текущему режиму. Спека R3 (матрица).
#[derive(Debug, PartialEq, Eq)]
pub enum ReopenPlan {
    OpenWrite,
    AlreadyWrite,
    AlreadyRead,
    RefuseUnsavedWrite,
}

/// READ→WRITE только: повторный open с диска молча выбросил бы несохранённые
/// мутации WRITE-образа. Спека R3.
pub fn reopen_plan(want_write: bool, current_write: bool) -> ReopenPlan {
    match (want_write, current_write) {
        (true, false) => ReopenPlan::OpenWrite,
        (true, true) => ReopenPlan::AlreadyWrite,
        (false, false) => ReopenPlan::AlreadyRead,
        (false, true) => ReopenPlan::RefuseUnsavedWrite,
    }
}

/// Цель :reopen/:w — строка-образ в Registry при фокусе там, иначе активный
/// образ. Спека R3.
pub fn reopen_target(app: &App) -> Option<String> {
    if app.focus == crate::app::Focus::Registry
        && let Some(crate::app::RegistryRow::Image(i)) = app.current_registry_row()
        && let Some(im) = app.registry.images.get(i)
    {
        return Some(im.image_id.clone());
    }
    app.active_image_id.clone()
}

/// Переоткрытие образа в write: open(WRITE) → switch → close(old) → refresh.
/// No-op ветки не делают RPC. Спека R3.
pub async fn reopen(
    app: &mut App,
    client: &mut Client,
    want_write: bool,
) -> Result<String, String> {
    let target_id = reopen_target(app).ok_or("no image selected (registry row or active)")?;
    let im = app
        .registry
        .images
        .iter()
        .find(|i| i.image_id == target_id)
        .cloned()
        .ok_or("image not in registry — :refresh")?;
    match reopen_plan(want_write, im.mode == 1) {
        ReopenPlan::AlreadyWrite => {
            app.status_msg = format!("already in write mode: {}", im.image_id);
            return Ok(im.image_id);
        }
        ReopenPlan::AlreadyRead => {
            app.status_msg = format!("already read-only: {}", im.image_id);
            return Ok(im.image_id);
        }
        ReopenPlan::RefuseUnsavedWrite => {
            return Err(format!(
                "refusing: unsaved write-mode image; :save first — {}",
                im.image_id
            ));
        }
        ReopenPlan::OpenWrite => {}
    }
    let sid = client.state.session_id.clone().ok_or("no session")?;
    let req = ImageOpenRequest {
        session_id: sid,
        path: im.path.clone(),
        mode: 1,
        name: String::new(),
    };
    let r = client
        .inner
        .image_open(auth_req(&client.state, req))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    let dump = client
        .inner
        .image_nodes_list(auth_req(
            &client.state,
            ImageNodesListRequest {
                image_id: r.image_id.clone(),
                filter: String::new(),
            },
        ))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    app.tree = crate::tree::build_tree(&dump.nodes);
    app.cursor = 0;
    app.active_image_id = Some(r.image_id.clone());
    client.state.active_image_id = Some(r.image_id.clone());
    let _ = client
        .inner
        .image_close(auth_req(
            &client.state,
            ImageCloseRequest {
                image_id: target_id,
            },
        ))
        .await;
    let _ = refresh_registry(app, client).await;
    if app.view == View::Forms {
        refresh_forms(app, client).await?;
    }
    app.status_msg = format!("reopened {} in write mode", im.name);
    Ok(r.image_id)
}

/// Команда для клавиши `v` на выбранной форме. Движок реализует только
/// unsuppress (visibility on); скрытие (off) — ошибка, поэтому на уже
/// видимой форме возвращает None — вызывающий показывает пояснение,
/// а не шлёт команду.
pub fn form_visibility_command(item: &str, visible: bool) -> Option<String> {
    (!visible).then(|| format!("hii visibility {item} on"))
}

/// Join u32-списков статусов; пустой — (none). Спека R7 (V3-мелочь 1).
fn fmt_u32_ids(ids: &[u32]) -> String {
    if ids.is_empty() {
        "(none)".into()
    } else {
        ids.iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn fmt_string_ids(ids: &std::collections::HashMap<String, u32>) -> String {
    if ids.is_empty() {
        return "(none)".into();
    }
    let mut v: Vec<(&String, &u32)> = ids.iter().collect();
    v.sort_by_key(|(name, _)| name.to_string());
    v.iter()
        .map(|(name, sid)| format!("{name}={sid}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Однострочный статус :hii formset add. Спека tui-forms-view §4 V3.
pub fn formset_add_status(
    new_ffs_id: &str,
    form_ids: &[u32],
    string_ids: &std::collections::HashMap<String, u32>,
) -> String {
    let forms = fmt_u32_ids(form_ids);
    format!(
        "formset added: ffs {new_ffs_id} · forms {forms} · strings {}",
        fmt_string_ids(string_ids)
    )
}

/// Однострочный статус :hii form add. Спека tui-forms-view §4 V3.
pub fn form_add_status(
    form_ids: &[u32],
    string_ids: &std::collections::HashMap<String, u32>,
) -> String {
    let forms = fmt_u32_ids(form_ids);
    format!(
        "form added: forms {forms} · strings {}",
        fmt_string_ids(string_ids)
    )
}

/// Однострочный двухфазный статус :hii import (спека hii-form-export
/// §3.4): form-фаза (поля как у form add) + ref-фаза — built (parent и
/// qid hex) / not built (причина). None-исход — пакет без refs-секции.
pub fn import_status(
    form_ids: &[u32],
    string_ids: &std::collections::HashMap<String, u32>,
    ref_outcome: Option<&Result<(u16, Vec<u16>), String>>,
) -> String {
    let mut s = format!(
        "import: forms {} · strings {}",
        fmt_u32_ids(form_ids),
        fmt_string_ids(string_ids)
    );
    match ref_outcome {
        None => {}
        Some(Ok((parent, qids))) => {
            let q = if qids.is_empty() {
                "(none)".to_string()
            } else {
                qids.iter()
                    .map(|q| format!("{q:#06x}"))
                    .collect::<Vec<_>>()
                    .join(",")
            };
            s.push_str(&format!(" · refs built under {parent}: {q}"));
        }
        Some(Err(cause)) => s.push_str(&format!(" · refs not built: {cause}")),
    }
    s
}

/// Однострочный статус :hii question add: qid в hex (конвенция q0xNNN),
/// string_ids всех исходов вперёд, сортировка по имени. Спека §4 V3.
pub fn question_add_status(
    questions: &[HiiQuestionAddOutcome],
    refs: &[HiiQuestionAddOutcome],
) -> String {
    let fmt_ids = |v: &[HiiQuestionAddOutcome]| {
        if v.is_empty() {
            "(none)".to_string()
        } else {
            v.iter()
                .map(|o| format!("{:#x}", o.question_id))
                .collect::<Vec<_>>()
                .join(",")
        }
    };
    let mut names: Vec<(String, u32)> = questions
        .iter()
        .chain(refs)
        .flat_map(|o| o.string_ids.iter().map(|(n, s)| (n.clone(), *s)))
        .collect();
    names.sort_by_key(|(n, _)| n.clone());
    let strings = if names.is_empty() {
        "(none)".into()
    } else {
        names
            .iter()
            .map(|(n, s)| format!("{n}={s}"))
            .collect::<Vec<_>>()
            .join(",")
    };
    format!(
        "question add: questions {} · refs {} · strings {strings}",
        fmt_ids(questions),
        fmt_ids(refs)
    )
}

/// Однострочный статус :hii page add (десятичные поля, как CLI). Спека §4 V3.
pub fn page_add_status(resp: &HiiPageAddResponse) -> String {
    format!(
        "page added: form {} · slot {} · offset {} · title sid {}",
        resp.form_id, resp.slot, resp.page_offset, resp.title_string_id
    )
}

/// Однострочный статус :hii hijack: flips join как unlock (V2), строки —
/// количеством (имён может быть много). Спека §4 V3.
pub fn hijack_status(resp: &HiiFormHijackResponse) -> String {
    let flips = if resp.unlock_flips.is_empty() {
        "none".to_string()
    } else {
        resp.unlock_flips.join(" · ")
    };
    format!(
        "hijack: ifr {:#x}..{:#x} · flips {flips} · strings {}",
        resp.form_ifr_start,
        resp.form_ifr_end,
        resp.string_ids.len()
    )
}

pub async fn refresh_forms(app: &mut App, client: &mut Client) -> Result<(), String> {
    let image_id = app
        .active_image_id
        .clone()
        .or_else(|| client.state.active_image_id.clone())
        .ok_or("no active image")?;
    let resp = client
        .inner
        .hii_list_forms(auth_req(
            &client.state,
            HiiListFormsRequest {
                image_id: image_id.clone(),
            },
        ))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    let edges = client
        .inner
        .hii_form_tree(auth_req(&client.state, HiiFormTreeRequest { image_id }))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner()
        .edges;
    app.forms.forms = resp.forms;
    app.forms.edges = edges;
    app.forms.expanded = crate::forms::all_row_keys(&app.forms.forms, &app.forms.edges);
    app.forms.cursor = 0;
    app.forms.questions.clear();
    app.forms.questions_key = None;
    app.forms.strings.clear();
    app.forms.strings_filter.clear();
    app.forms.strings_cursor = 0;
    app.forms.show_strings = false;
    app.forms.varstores.clear();
    app.forms.varstores_target = None;
    app.forms.varstores_cursor = 0;
    app.forms.show_varstores = false;
    Ok(())
}

/// Re-fetch форм И рёбер после мутации: сохраняет flat_mode,
/// развёрнутость, выделение (по formset+form_id), strings-браузер;
/// сбрасывает per-form кэши (questions, gates, question_info) и
/// varstores-кэш (target — панель остаётся, 'V' re-fetch'ит).
/// Вход во view — refresh_forms (сброс), не эта функция.
/// Спека tui-forms-view §3.3.
pub async fn reload_forms(app: &mut App, client: &mut Client) -> Result<(), String> {
    let image_id = app
        .active_image_id
        .clone()
        .or_else(|| client.state.active_image_id.clone())
        .ok_or("no active image")?;
    let sel = app
        .selected_form_key()
        .map(|k| (k.formset_guid, k.form_id_ifr));
    let forms = client
        .inner
        .hii_list_forms(auth_req(
            &client.state,
            HiiListFormsRequest {
                image_id: image_id.clone(),
            },
        ))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner()
        .forms;
    let edges = client
        .inner
        .hii_form_tree(auth_req(&client.state, HiiFormTreeRequest { image_id }))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner()
        .edges;
    app.forms.forms = forms;
    app.forms.edges = edges;
    app.forms.questions.clear();
    app.forms.questions_key = None;
    app.forms.gates.clear();
    app.forms.question_cursor = 0;
    app.forms.question_info = None;
    app.forms.question_info_key = None;
    app.forms.varstores_target = None;
    match sel {
        Some((guid, fid)) => {
            let rows = app.forms_rows();
            let idx = rows.iter().position(|r| {
                matches!(
                    r,
                    crate::forms::FormsRow::Form { key, .. }
                        if key.formset_guid == guid && key.form_id_ifr == fid
                )
            });
            match idx {
                Some(i) => app.forms.cursor = i,
                None => app.forms_sanitize_cursor(),
            }
        }
        None => app.forms_sanitize_cursor(),
    }
    Ok(())
}

/// Вопросы И гейты выделенной формы — лениво, по смене FormKey
/// (кэш на одну форму). Ошибка gates-запроса не валит вопросы:
/// текст в status_msg, список гейтов пуст. Спека tui-forms-view
/// §4 V2.
pub async fn refresh_form_details_if_needed(
    app: &mut App,
    client: &mut Client,
) -> Result<(), String> {
    let Some(key) = app.selected_form_key() else {
        return Ok(());
    };
    if app.forms.questions_key.as_ref() == Some(&key) {
        return Ok(());
    }
    let image_id = app
        .active_image_id
        .clone()
        .or_else(|| client.state.active_image_id.clone())
        .ok_or("no active image")?;
    let resp = client
        .inner
        .hii_list_questions(auth_req(
            &client.state,
            HiiListQuestionsRequest {
                image_id: image_id.clone(),
                target: key.target.clone(),
                form_id: key.form_id_ifr,
            },
        ))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    app.forms.questions = resp.questions;
    app.forms.questions_key = Some(key.clone());
    app.forms.question_cursor = 0;
    app.forms.question_info = None;
    app.forms.question_info_key = None;
    let item_id = fmt_item(&key.target, key.form_id_ifr);
    match client
        .inner
        .hii_gates_list(auth_req(
            &client.state,
            HiiGatesListRequest { image_id, item_id },
        ))
        .await
    {
        Ok(r) => app.forms.gates = r.into_inner().gates,
        Err(e) => {
            app.forms.gates.clear();
            app.status_msg = format!("gates: {}", e.message());
        }
    }
    Ok(())
}

/// Переменные NVAR-стора под курсором Image View — лениво, кэш по
/// (image_id, path); не-стор очищает панель. Спека nvar-op §7.
pub async fn refresh_nvars_if_needed(app: &mut App, client: &mut Client) -> Result<(), String> {
    let Some(path) = app.selected_path() else {
        return Ok(());
    };
    if !app.selected_is_nvar() {
        if app.nvar.key.is_some() || !app.nvar.stores.is_empty() {
            app.nvar = Default::default();
        }
        return Ok(());
    }
    let image_id = app
        .active_image_id
        .clone()
        .or_else(|| client.state.active_image_id.clone())
        .ok_or("no active image")?;
    let key = format!("{image_id}:{path}");
    if app.nvar.key.as_deref() == Some(&key) {
        return Ok(());
    }
    let resp = client
        .inner
        .nvar_list(auth_req(
            &client.state,
            NvarListRequest {
                image_id,
                path: Some(path),
                include_data: true,
            },
        ))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    app.nvar.stores = resp.stores;
    app.nvar.cursor = 0;
    app.nvar.hex_scroll = 0;
    app.nvar.list_state = ratatui::widgets::ListState::default();
    app.nvar.key = Some(key);
    Ok(())
}

/// Диапазон/options выбранного вопроса — лениво, по смене
/// (FormKey, question_id); кэш на один вопрос. Спека tui-forms-view
/// §4 V2 (подсказка при вводе set-value).
pub async fn refresh_question_info_if_needed(
    app: &mut App,
    client: &mut Client,
) -> Result<(), String> {
    let Some(key) = app.selected_form_key() else {
        return Ok(());
    };
    let Some(qid) = app.selected_question_id() else {
        app.forms.question_info = None;
        app.forms.question_info_key = None;
        return Ok(());
    };
    if app.forms.question_info_key.as_ref() == Some(&(key.clone(), qid)) {
        return Ok(());
    }
    let image_id = app
        .active_image_id
        .clone()
        .or_else(|| client.state.active_image_id.clone())
        .ok_or("no active image")?;
    let item_id = format!("{}#{}:{:#x}", key.target, key.form_id_ifr, qid);
    let resp = client
        .inner
        .hii_question_info(auth_req(
            &client.state,
            HiiQuestionInfoRequest { image_id, item_id },
        ))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    app.forms.question_info = resp.question;
    app.forms.question_info_key = Some((key, qid));
    Ok(())
}

pub async fn refresh_strings(app: &mut App, client: &mut Client) -> Result<(), String> {
    let image_id = app
        .active_image_id
        .clone()
        .or_else(|| client.state.active_image_id.clone())
        .ok_or("no active image")?;
    let resp = client
        .inner
        .hii_list_strings(auth_req(&client.state, HiiListStringsRequest { image_id }))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    app.forms.strings = resp.strings;
    app.forms.strings_cursor = 0;
    Ok(())
}

/// Карта varstore-деклараций формсета под курсором — лениво, кэш на
/// один target. Спека varstore-contract §5.
pub async fn refresh_varstores(
    app: &mut App,
    client: &mut Client,
    target: &str,
) -> Result<(), String> {
    let image_id = app
        .active_image_id
        .clone()
        .or_else(|| client.state.active_image_id.clone())
        .ok_or("no active image")?;
    let resp = client
        .inner
        .hii_list_varstores(auth_req(
            &client.state,
            HiiListVarstoresRequest {
                image_id,
                target: target.into(),
            },
        ))
        .await
        .map_err(|e| e.message().to_string())?
        .into_inner();
    app.forms.varstores = resp.varstores;
    app.forms.varstores_target = Some(target.to_string());
    app.forms.varstores_cursor = 0;
    Ok(())
}

pub async fn restore_session(app: &mut App, client: &mut Client) -> Result<(), String> {
    if let Err(e) = refresh_registry(app, client).await {
        app.status_msg = format!("registry: {e}");
    }
    if let Some(id) = client.state.active_image_id.clone() {
        let req = ImageNodesListRequest {
            image_id: id.clone(),
            filter: String::new(),
        };
        match client
            .inner
            .image_nodes_list(auth_req(&client.state, req))
            .await
        {
            Ok(resp) => {
                let dump = resp.into_inner();
                app.tree = crate::tree::build_tree(&dump.nodes);
                app.cursor = 0;
                app.active_image_id = Some(id.clone());
                client.state.active_image_id = Some(id);
                app.image_loaded = true;
                app.sanitize_cursor();
                app.status_msg = "restored".into();
            }
            Err(e) => {
                let msg = e.message().to_string();
                app.active_image_id = None;
                client.state.active_image_id = None;
                app.status_msg = format!("active image unavailable: {msg}");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_first_token_to_unique_command() {
        let app = crate::app::App::new();
        let c = complete(&app, "rebui");
        assert_eq!(c.common.as_deref(), Some("rebuild "));
        assert!(c.items.is_empty());
    }

    #[test]
    fn commands_const_has_top_level_switch_close() {
        assert!(COMMANDS.contains(&"switch"));
        assert!(COMMANDS.contains(&"close"));
        assert!(
            COMMANDS.contains(&"image"),
            "bare :image остаётся view-переключателем"
        );
    }

    #[test]
    fn reopen_plan_guard_matrix() {
        use ReopenPlan::*;
        assert_eq!(super::reopen_plan(true, false), OpenWrite);
        assert_eq!(super::reopen_plan(true, true), AlreadyWrite);
        assert_eq!(super::reopen_plan(false, false), AlreadyRead);
        assert_eq!(super::reopen_plan(false, true), RefuseUnsavedWrite);
    }

    #[test]
    fn complete_artifact_ids_after_flag() {
        let mut app = crate::app::App::new();
        app.registry.artifacts = vec![
            uefi_proto::ArtifactInfo {
                artifact_id: "art-1".into(),
                ..Default::default()
            },
            uefi_proto::ArtifactInfo {
                artifact_id: "art-2".into(),
                ..Default::default()
            },
        ];
        let c = complete(&app, "insert 0/3 --artifact-id art-");
        assert_eq!(c.common.as_deref(), Some("insert 0/3 --artifact-id art-"));
        let c = complete(&app, "insert 0/3 --artifact-id ");
        assert_eq!(
            c.items
                .iter()
                .map(|i| i.display.clone())
                .collect::<Vec<_>>(),
            vec!["art-1".to_string(), "art-2".to_string()]
        );
    }

    #[test]
    fn complete_switch_close_offer_registry_image_ids() {
        let mut app = crate::app::App::new();
        app.registry.images = vec![
            uefi_proto::ImageInfo {
                image_id: "img-aaa".into(),
                ..Default::default()
            },
            uefi_proto::ImageInfo {
                image_id: "img-bbb".into(),
                ..Default::default()
            },
        ];
        for cmd in ["switch ", "close "] {
            let c = complete(&app, cmd);
            assert_eq!(
                c.items
                    .iter()
                    .map(|i| i.display.clone())
                    .collect::<Vec<_>>(),
                vec!["img-aaa".to_string(), "img-bbb".to_string()],
                "слот-1 {cmd}"
            );
        }
        let c = complete(&app, "switch img-a");
        assert_eq!(c.common.as_deref(), Some("switch img-aaa "));
    }

    #[test]
    fn complete_mode_values_depend_on_command() {
        let app = crate::app::App::new();
        let c = complete(&app, "insert 0/3 --mode ");
        let vals: Vec<_> = c.items.iter().map(|i| i.display.clone()).collect();
        assert!(vals.contains(&"into".to_string()));
        let c = complete(&app, "reopen --mode ");
        let vals: Vec<_> = c.items.iter().map(|i| i.display.clone()).collect();
        assert_eq!(vals, vec!["read".to_string(), "write".to_string()]);
        let c = complete(&app, "reopen --");
        assert_eq!(c.common.as_deref(), Some("reopen --mode "));
        let c = complete(&app, "open /x --mode ");
        let vals: Vec<_> = c.items.iter().map(|i| i.display.clone()).collect();
        assert_eq!(
            vals,
            vec!["read".to_string(), "write".to_string()],
            ":open PATH --mode read|write — как в usage"
        );
        let c = complete(&app, "open /x --");
        assert_eq!(c.common.as_deref(), Some("open /x --mode "));
    }

    #[test]
    fn complete_flags_of_insert() {
        let app = crate::app::App::new();
        let c = complete(&app, "insert 0/3 --");
        assert_eq!(
            c.items
                .iter()
                .map(|i| i.display.clone())
                .collect::<Vec<_>>(),
            vec![
                "--file".to_string(),
                "--artifact-id".to_string(),
                "--mode".to_string()
            ]
        );
    }

    #[test]
    fn complete_extract_offers_body_only_flag() {
        let app = crate::app::App::new();
        let c = complete(&app, "extract 1/3 --bod");
        assert_eq!(c.common.as_deref(), Some("extract 1/3 --body-only "));
        let c = complete(&app, "extract 1/3 --");
        assert_eq!(
            c.common.as_deref(),
            Some("extract 1/3 --body-only "),
            "единственный флаг — через common, items пуст (паттерн single-candidate)"
        );
    }

    #[test]
    fn complete_o_alias_offers_mode_flag() {
        let app = crate::app::App::new();
        let c = complete(&app, "o p.bin --mo");
        assert_eq!(
            c.common.as_deref(),
            Some("o p.bin --mode "),
            "алиас :o получает флаги :open (single-candidate: common, items пуст)"
        );
        let c = complete(&app, "o p.bin --");
        assert_eq!(c.common.as_deref(), Some("o p.bin --mode "));
        let c = complete(&app, "o p.bin --mode re");
        assert_eq!(c.common.as_deref(), Some("o p.bin --mode read "));
    }

    #[test]
    fn parse_u64_loose_hex_and_dec() {
        assert_eq!(parse_u64_loose("0x1").unwrap(), 1);
        assert_eq!(parse_u64_loose("0xFF").unwrap(), 255);
        assert_eq!(parse_u64_loose("42").unwrap(), 42);
        assert!(parse_u64_loose("0xG").is_err());
        assert!(parse_u64_loose("").is_err());
    }

    #[test]
    fn absolutize_path_relative_to_client_cwd() {
        let abs = absolutize_path("/tmp/x.bin");
        assert_eq!(abs, "/tmp/x.bin");
        let rel = absolutize_path("out.bin");
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(rel, cwd.join("out.bin").display().to_string());
    }

    #[test]
    fn export_output_path_absolute_or_cwd_default() {
        let abs = export_output_path(Some("/tmp/out.bin"), "art-1");
        assert_eq!(abs, "/tmp/out.bin");
        let rel = export_output_path(Some("out.bin"), "art-1");
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(rel, cwd.join("out.bin").display().to_string());
        let def = export_output_path(None, "art-1");
        assert_eq!(
            def,
            cwd.join("art-1").display().to_string(),
            "дефолт — файл, не каталог"
        );
    }

    #[test]
    fn form_visibility_command_only_unsuppresses() {
        assert_eq!(
            form_visibility_command("G:0x19:0#42", false).as_deref(),
            Some("hii visibility G:0x19:0#42 on")
        );
        assert_eq!(
            form_visibility_command("G:0x19:0#42", true),
            None,
            "скрытие не поддержано движком — на видимой форме команды нет"
        );
    }

    #[test]
    fn schema_status_formats() {
        let mut sids = std::collections::HashMap::new();
        sids.insert("title".to_string(), 600);
        sids.insert("help".to_string(), 601);
        assert_eq!(
            formset_add_status("ffs-9", &[10101, 10102], &sids),
            "formset added: ffs ffs-9 · forms 10101,10102 · strings help=601,title=600"
        );
        assert_eq!(
            formset_add_status("ffs-9", &[], &std::collections::HashMap::new()),
            "formset added: ffs ffs-9 · forms (none) · strings (none)"
        );
        assert_eq!(
            form_add_status(&[10101], &sids),
            "form added: forms 10101 · strings help=601,title=600"
        );
        assert_eq!(
            form_add_status(&[], &std::collections::HashMap::new()),
            "form added: forms (none) · strings (none)"
        );
    }

    #[test]
    fn question_add_status_hex_qids_and_strings() {
        let mk = |qid: u32, sids: &[(&str, u32)]| HiiQuestionAddOutcome {
            question_id: qid,
            string_ids: sids.iter().map(|(n, s)| (n.to_string(), *s)).collect(),
            spf_record_offset: 0x1C,
        };
        assert_eq!(
            question_add_status(&[mk(0x258, &[("prompt", 600)])], &[]),
            "question add: questions 0x258 · refs (none) · strings prompt=600"
        );
        assert_eq!(
            question_add_status(
                &[mk(0x258, &[])],
                &[mk(0x25A, &[("prompt", 600), ("help", 601)])]
            ),
            "question add: questions 0x258 · refs 0x25a · strings help=601,prompt=600"
        );
        assert_eq!(
            question_add_status(&[], &[]),
            "question add: questions (none) · refs (none) · strings (none)"
        );
    }

    #[test]
    fn page_and_hijack_status() {
        let page = HiiPageAddResponse {
            form_id: 10019,
            slot: 1,
            page_offset: 42,
            title_string_id: 600,
        };
        assert_eq!(
            page_add_status(&page),
            "page added: form 10019 · slot 1 · offset 42 · title sid 600"
        );
        let mut sids = std::collections::HashMap::new();
        sids.insert("title".to_string(), 600);
        let hijack = HiiFormHijackResponse {
            string_ids: sids,
            form_ifr_start: 0x1000,
            form_ifr_end: 0x1100,
            unlock_flips: vec!["pkg+0x1c: 01 00 -> ff ff".into()],
            ..Default::default()
        };
        assert_eq!(
            hijack_status(&hijack),
            "hijack: ifr 0x1000..0x1100 · flips pkg+0x1c: 01 00 -> ff ff · strings 1"
        );
        let no_flips = HiiFormHijackResponse::default();
        assert_eq!(
            hijack_status(&no_flips),
            "hijack: ifr 0x0..0x0 · flips none · strings 0"
        );
    }

    #[test]
    fn complete_hii_verbs_and_item_ids() {
        let mut app = crate::app::App::new();
        app.forms.forms = vec![uefi_proto::FormInfo {
            form_id: "t:0x19:0".into(),
            formset_guid: "S".into(),
            form_id_ifr: 10001,
            title: "Main".into(),
            visible: true,
        }];
        let c = complete(&app, "hii ");
        assert_eq!(
            c.items
                .iter()
                .map(|i| i.display.clone())
                .collect::<Vec<_>>(),
            vec![
                "formset".to_string(),
                "form".to_string(),
                "question".to_string(),
                "page".to_string(),
                "hijack".to_string(),
                "set-value".to_string(),
                "visibility".to_string(),
                "unlock".to_string(),
                "import".to_string()
            ]
        );
        let c = complete(&app, "hii set-value ");
        assert_eq!(c.common.as_deref(), Some("hii set-value t:0x19:0#10001 "));
        assert!(
            c.items.is_empty(),
            "unique candidate completes directly, no menu"
        );
    }

    #[test]
    fn complete_hii_nouns_add_targets_items_and_paths() {
        let mut app = crate::app::App::new();
        app.forms.forms = vec![
            uefi_proto::FormInfo {
                form_id: "t:0x19:0".into(),
                formset_guid: "SET-A".into(),
                form_id_ifr: 10001,
                title: "Main".into(),
                visible: true,
            },
            uefi_proto::FormInfo {
                form_id: "t:0x19:0".into(),
                formset_guid: "SET-A".into(),
                form_id_ifr: 10019,
                title: "Serial".into(),
                visible: false,
            },
        ];
        let c = complete(&app, "hii form ");
        assert_eq!(
            c.items
                .iter()
                .map(|i| i.display.clone())
                .collect::<Vec<_>>(),
            vec!["add".to_string(), "export".to_string()],
            "form — два глагола: add и export (hii-form-export §4)"
        );
        let c = complete(&app, "hii form a");
        assert_eq!(
            c.common.as_deref(),
            Some("hii form add "),
            "единственный кандидат инлайн-дополняется (контракт complete)"
        );
        let c = complete(&app, "hii formset ");
        assert_eq!(c.common.as_deref(), Some("hii formset add "));

        let c = complete(&app, "hii form add ");
        assert_eq!(
            c.common.as_deref(),
            Some("hii form add t:0x19:0 "),
            "голые target-кандидаты, дедуп по двум формам одного target"
        );
        let c = complete(&app, "hii hijack ");
        assert_eq!(c.common.as_deref(), Some("hii hijack t:0x19:0 "));
        let c = complete(&app, "hii question add ");
        assert_eq!(
            c.items
                .iter()
                .map(|i| i.display.clone())
                .collect::<Vec<_>>(),
            vec!["t:0x19:0#10001".to_string(), "t:0x19:0#10019".to_string()],
            "question add — item-кандидаты target#form_id (form_id десятичное)"
        );
    }

    #[test]
    fn unique_candidate_gets_trailing_space() {
        let app = crate::app::App::new();
        let c = complete(&app, "hi");
        assert_eq!(c.common.as_deref(), Some("hii "));
        assert!(c.items.is_empty());
    }

    #[test]
    fn open_completes_paths_in_slot1() {
        let td = tempfile::tempdir().unwrap();
        std::fs::create_dir(td.path().join("real")).unwrap();
        std::fs::write(td.path().join("lite"), b"").unwrap();
        std::os::unix::fs::symlink(td.path().join("real"), td.path().join("link")).unwrap();
        let base = td.path().display().to_string();
        let c = complete(&crate::app::App::new(), &format!("open {base}/li"));
        let applies: Vec<&str> = c.items.iter().map(|i| i.apply.as_str()).collect();
        assert!(
            applies
                .iter()
                .any(|a| a.ends_with("/link/") && !a.ends_with("/link/ "))
        );
        assert!(applies.iter().any(|a| a.ends_with("lite ")));
    }

    #[test]
    fn file_flag_value_completes_paths() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("schema-a.json"), b"{}").unwrap();
        let base = td.path().display().to_string();
        let c = complete(
            &crate::app::App::new(),
            &format!("insert --file {base}/schema-a"),
        );
        assert_eq!(
            c.common.as_deref(),
            Some(format!("insert --file {base}/schema-a.json ").as_str())
        );
    }

    #[test]
    fn expand_tilde_variants() {
        assert_eq!(expand_tilde("~", "/home/u"), "/home/u");
        assert_eq!(expand_tilde("~/x/y", "/home/u"), "/home/u/x/y");
        assert_eq!(expand_tilde("x/~", "/home/u"), "x/~");
        assert_eq!(expand_tilde("", "/home/u"), "");
    }

    #[test]
    fn export_completes_artifact_ids_then_path() {
        let mut app = crate::app::App::new();
        app.registry.artifacts = vec![
            uefi_proto::ArtifactInfo {
                artifact_id: "art-1".into(),
                ..Default::default()
            },
            uefi_proto::ArtifactInfo {
                artifact_id: "art-2".into(),
                ..Default::default()
            },
        ];
        let c = complete(&app, "export art");
        assert_eq!(
            c.items
                .iter()
                .map(|i| i.display.clone())
                .collect::<Vec<_>>(),
            vec!["art-1".to_string(), "art-2".to_string()]
        );
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("blob.bin"), b"").unwrap();
        let base = td.path().display().to_string();
        let c = complete(&app, &format!("export art-1 {base}/bl"));
        assert_eq!(
            c.common.as_deref(),
            Some(format!("export art-1 {base}/blob.bin ").as_str())
        );
    }

    #[test]
    fn import_completes_paths_in_slot1() {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("blob.bin"), b"").unwrap();
        let base = td.path().display().to_string();
        let c = complete(&crate::app::App::new(), &format!("import {base}/bl"));
        assert_eq!(
            c.common.as_deref(),
            Some(format!("import {base}/blob.bin ").as_str())
        );
    }

    #[test]
    fn complete_path_lists_dir_entries_with_slash_for_dirs() {
        let td = tempfile::TempDir::new().unwrap();
        std::fs::write(td.path().join("schema-a.json"), "{}").unwrap();
        std::fs::write(td.path().join("schema-b.json"), "{}").unwrap();
        std::fs::write(td.path().join(".hidden.json"), "{}").unwrap();
        std::fs::create_dir(td.path().join("fixtures")).unwrap();
        let base = td.path().display().to_string();
        assert_eq!(
            complete_path(&format!("{base}/schema-")),
            vec![
                format!("{base}/schema-a.json"),
                format!("{base}/schema-b.json")
            ]
        );
        assert_eq!(
            complete_path(&format!("{base}/fix")),
            vec![format!("{base}/fixtures/")],
            "каталог получает / — следующий Tab спускается внутрь"
        );
        assert_eq!(
            complete_path(&format!("{base}/.")),
            vec![format!("{base}/.hidden.json")],
            "скрытые — только по явному точечному префиксу"
        );
        assert!(complete_path(&format!("{base}/no-such-dir-9f1/x")).is_empty());
    }

    #[test]
    fn complete_hii_add_file_position_is_path_completion() {
        let td = tempfile::TempDir::new().unwrap();
        std::fs::write(td.path().join("schema-x.json"), "{}").unwrap();
        let base = td.path().display().to_string();
        let mut app = crate::app::App::new();
        app.forms.forms = vec![uefi_proto::FormInfo {
            form_id: "t:0x19:0".into(),
            formset_guid: "SET-A".into(),
            form_id_ifr: 10001,
            title: "Main".into(),
            visible: true,
        }];
        let c = complete(&app, &format!("hii formset add {base}/schema-x"));
        assert_eq!(
            c.common.as_deref(),
            Some(format!("hii formset add {base}/schema-x.json ").as_str())
        );
        let c = complete(&app, &format!("hii form add t:0x19:0 {base}/schema-x"));
        assert_eq!(
            c.common.as_deref(),
            Some(format!("hii form add t:0x19:0 {base}/schema-x.json ").as_str())
        );
        let c = complete(&app, &format!("hii hijack t:0x19:0 {base}/schema-x"));
        assert_eq!(
            c.common.as_deref(),
            Some(format!("hii hijack t:0x19:0 {base}/schema-x.json ").as_str())
        );
    }

    #[test]
    fn complete_ffs_flag_and_formset_guid_value() {
        let mut app = crate::app::App::new();
        app.forms.forms = vec![uefi_proto::FormInfo {
            form_id: "t:0x19:0".into(),
            formset_guid: "SET-A".into(),
            form_id_ifr: 10001,
            title: "Main".into(),
            visible: true,
        }];
        let c = complete(&app, "hii formset add f.json --");
        assert_eq!(c.common.as_deref(), Some("hii formset add f.json --ffs "));
        let c = complete(&app, "hii formset add f.json --ffs ");
        assert_eq!(
            c.common.as_deref(),
            Some("hii formset add f.json --ffs SET-A ")
        );
    }

    #[test]
    fn ffs_flag_only_in_grammar_position() {
        let app = crate::app::App::new();
        let c = complete(&app, "hii formset add f.json --f");
        assert_eq!(c.common.as_deref(), Some("hii formset add f.json --ffs "));
        let c = complete(&app, "hii formset add --f");
        assert_eq!(c.common.as_deref(), None);
        let c = complete(&app, "hii form add x --f");
        assert_ne!(c.common.as_deref(), Some("hii form add x --ffs"));
    }

    #[cfg(unix)]
    #[test]
    fn complete_path_descends_into_dir_symlinks() {
        let td = tempfile::tempdir().unwrap();
        std::fs::create_dir(td.path().join("real")).unwrap();
        std::os::unix::fs::symlink(td.path().join("real"), td.path().join("link")).unwrap();
        let base = td.path().display().to_string();
        assert_eq!(
            complete_path(&format!("{base}/li")),
            vec![format!("{base}/link/")]
        );
    }

    #[test]
    fn add_prefill_formset_form_and_dangling() {
        let mut app = crate::app::App::new();
        app.forms.forms = vec![uefi_proto::FormInfo {
            form_id: "11111111-2222-3333-4444-555555555555:0x19:0".into(),
            formset_guid: "SET-A".into(),
            form_id_ifr: 10001,
            title: "Main".into(),
            visible: true,
        }];
        app.forms.edges = vec![uefi_proto::FormEdge {
            formset_guid: "SET-A".into(),
            parent_form_id: 10001,
            form_id: 99,
            target_formset_guid: String::new(),
        }];
        app.forms.expanded = crate::forms::all_row_keys(&app.forms.forms, &app.forms.edges);
        app.forms.cursor = 0;
        assert_eq!(
            add_prefill(&app).as_deref(),
            Some("hii formset add "),
            "FormSet-строка: formset add не требует target"
        );
        app.forms.cursor = 1;
        assert_eq!(
            add_prefill(&app).as_deref(),
            Some("hii form add 11111111-2222-3333-4444-555555555555:0x19:0 ")
        );
        app.forms.cursor = 2;
        assert_eq!(
            add_prefill(&app),
            None,
            "DanglingRef — не форма и не формсет, prefill нет"
        );
    }

    #[test]
    fn fmt_u32_ids_join_and_none() {
        assert_eq!(fmt_u32_ids(&[]), "(none)");
        assert_eq!(fmt_u32_ids(&[10029, 10057]), "10029,10057");
    }

    #[test]
    fn add_prefill_none_when_no_forms() {
        let app = crate::app::App::new();
        assert!(
            add_prefill(&app).is_none(),
            "пустой forms-список — префилла нет"
        );
    }

    fn hii_key_app() -> crate::app::App {
        let mut app = crate::app::App::new();
        app.forms.forms = vec![
            uefi_proto::FormInfo {
                form_id: "t1:0x19:0".into(),
                formset_guid: "SET-A".into(),
                form_id_ifr: 10001,
                title: "Main".into(),
                visible: true,
            },
            uefi_proto::FormInfo {
                form_id: "t1:0x19:0".into(),
                formset_guid: "SET-A".into(),
                form_id_ifr: 10019,
                title: "Serial".into(),
                visible: false,
            },
            uefi_proto::FormInfo {
                form_id: "t2:0x19:0".into(),
                formset_guid: "SET-B".into(),
                form_id_ifr: 902,
                title: "Platform".into(),
                visible: true,
            },
        ];
        app.forms.edges = vec![uefi_proto::FormEdge {
            formset_guid: "SET-A".into(),
            parent_form_id: 10001,
            form_id: 99,
            target_formset_guid: String::new(),
        }];
        app.forms.expanded = crate::forms::all_row_keys(&app.forms.forms, &app.forms.edges);
        app
    }

    #[test]
    fn export_prefill_on_form_rows_only() {
        let mut app = hii_key_app();
        app.forms.cursor = 1;
        assert_eq!(
            export_prefill(&app).as_deref(),
            Some("hii form export t1:0x19:0#10001 --out ")
        );
        app.forms.cursor = 0;
        assert!(export_prefill(&app).is_none(), "FormSet-строка — не форма");
        app.forms.cursor = 2;
        assert!(export_prefill(&app).is_none(), "DanglingRef — prefill нет");
    }

    #[test]
    fn import_prefill_target_from_formset_row_only() {
        let mut app = hii_key_app();
        app.forms.cursor = 0;
        assert_eq!(
            import_prefill(&app),
            "hii import t1:0x19:0 ",
            "target формсета — form_id первой его формы"
        );
        app.forms.cursor = 4;
        assert_eq!(
            import_prefill(&app),
            "hii import t2:0x19:0 ",
            "второй формсет — свой target"
        );
        app.forms.cursor = 1;
        assert_eq!(
            import_prefill(&app),
            "hii import ",
            "Form-строка — bare (план: target только с FormSet-строки)"
        );
        let empty = crate::app::App::new();
        assert_eq!(import_prefill(&empty), "hii import ");
    }

    #[test]
    fn question_add_prefill_on_form_rows_only() {
        let mut app = hii_key_app();
        app.forms.cursor = 3;
        assert_eq!(
            question_add_prefill(&app).as_deref(),
            Some("hii question add t1:0x19:0#10019 "),
            "form_id десятичное — контракт parse_item_id"
        );
        app.forms.cursor = 0;
        assert!(question_add_prefill(&app).is_none(), "FormSet-строка");
        app.forms.cursor = 2;
        assert!(question_add_prefill(&app).is_none(), "DanglingRef");
    }

    #[test]
    fn ref_import_prefill_no_parent_and_hint() {
        let key = crate::forms::FormKey {
            target: "t1:0x19:0".into(),
            formset_guid: "SET-A".into(),
            form_id_ifr: 5002,
            title: "X".into(),
        };
        assert_eq!(
            ref_import_prefill(&key),
            "hii import t1:0x19:0 --file refs.json",
            "import без #parent (docs:fix 001ad0a): родитель — refs.parent_form_id пакета"
        );
        let hint = ref_import_hint(&key);
        assert!(hint.contains("5002"), "form_id строки под курсором: {hint}");
        assert!(hint.contains("SET-A"), "formset_guid строки: {hint}");
    }

    #[test]
    fn export_default_path_dir_form_id_and_fallbacks() {
        assert_eq!(
            export_default_path_in("/tmp", "t:0x19:0#10019"),
            "/tmp/uefipatcher-export-10019.json"
        );
        assert_eq!(
            export_default_path_in("/tmp", "t:0x19:0#10019:0x210"),
            "/tmp/uefipatcher-export-10019.json",
            "вопросный суффикс :qid игнорируется"
        );
        assert_eq!(
            export_default_path_in("/tmp", "nohash"),
            "/tmp/uefipatcher-export-form.json"
        );
        assert_eq!(
            export_default_path_in("", "t#7"),
            "/tmp/uefipatcher-export-7.json",
            "пустой TMPDIR — fallback /tmp"
        );
    }

    #[test]
    fn refs_from_parent_zero_none_and_overflow() {
        assert!(refs_from_parent(0, &[]).unwrap().is_none());
        let refs = refs_from_parent(
            10001,
            &[uefi_proto::HiiFormExportEntry {
                prompt: "PCI Subsystem Settings".into(),
                help: "Open PCI subsystem settings".into(),
            }],
        )
        .unwrap()
        .unwrap();
        assert_eq!(refs.parent_form_id, 10001);
        assert_eq!(
            refs.entries,
            vec![uefi_common::envelope::RefEntry {
                prompt: Some("PCI Subsystem Settings".into()),
                help: Some("Open PCI subsystem settings".into()),
                ..Default::default()
            }],
            "entries — prompt/help GOTO родителя из ответа (спека §2)"
        );
        assert!(refs_from_parent(u16::MAX as u32 + 1, &[]).is_err());
    }

    #[test]
    fn import_status_two_phase_variants() {
        let mut sids = std::collections::HashMap::new();
        sids.insert("title".to_string(), 600);
        assert_eq!(
            import_status(&[10101], &sids, None),
            "import: forms 10101 · strings title=600",
            "пакет без refs-секции — только form-фаза"
        );
        assert_eq!(
            import_status(
                &[],
                &std::collections::HashMap::new(),
                Some(&Ok((10001, vec![0x7F00])))
            ),
            "import: forms (none) · strings (none) · refs built under 10001: 0x7f00",
            "refs-only: qid в hex"
        );
        assert_eq!(
            import_status(
                &[7],
                &std::collections::HashMap::new(),
                Some(&Err("boom".into()))
            ),
            "import: forms 7 · strings (none) · refs not built: boom",
            "падение ref-фазы не прерывает отчёт (§3.4)"
        );
    }

    #[test]
    fn import_prechecks_mirror_cli_semantics() {
        let form = |id: &str, set: &str, fid: u32| uefi_proto::FormInfo {
            form_id: id.into(),
            formset_guid: set.into(),
            form_id_ifr: fid,
            title: String::new(),
            visible: true,
        };
        let forms = vec![form("g:0x19:0", "G-1", 1), form("other:0x10:0", "G-2", 7)];
        assert!(check_parent(&forms, "g:0x19:0", 1).is_ok());
        assert!(check_parent(&forms, "g:0x19:0", 9).is_err());
        let refs = uefi_common::envelope::RefsSection {
            parent_form_id: 1,
            entries: vec![
                uefi_common::envelope::RefEntry {
                    question_id: Some(0x22),
                    ..Default::default()
                },
                uefi_common::envelope::RefEntry {
                    form_id: Some(7),
                    formset_guid: Some("G-2".into()),
                    ..Default::default()
                },
            ],
        };
        assert!(check_qids(&refs, &[0x23]).is_ok());
        assert!(
            check_qids(&refs, &[0x22])
                .unwrap_err()
                .contains("already busy")
        );
        assert!(check_targets(&forms, "g:0x19:0", &refs).is_ok());
        let dangling = uefi_common::envelope::RefsSection {
            parent_form_id: 1,
            entries: vec![uefi_common::envelope::RefEntry {
                form_id: Some(6000),
                ..Default::default()
            }],
        };
        assert!(check_targets(&forms, "g:0x19:0", &dangling).is_err());
        let qs = vec![uefi_proto::QuestionSummary {
            question_id: 0x210,
            ..Default::default()
        }];
        assert_eq!(busy_qids(&qs).unwrap(), vec![0x210]);
        let overflow = vec![uefi_proto::QuestionSummary {
            question_id: 0x10000,
            ..Default::default()
        }];
        assert!(busy_qids(&overflow).is_err());
        let stores = vec![uefi_proto::VarStoreInfo {
            id: 2,
            guid: "G".into(),
            size: 0x94,
            name: "Setup".into(),
        }];
        assert_eq!(varstore_briefs(&stores).unwrap()[0].id, 2);
    }

    #[test]
    fn complete_hii_form_two_verbs_add_and_export() {
        let app = hii_key_app();
        let c = complete(&app, "hii form ");
        assert_eq!(
            c.items
                .iter()
                .map(|i| i.display.clone())
                .collect::<Vec<_>>(),
            vec!["add".to_string(), "export".to_string()]
        );
        let c = complete(&app, "hii form e");
        assert_eq!(c.common.as_deref(), Some("hii form export "));
        let c = complete(&app, "hii formset ");
        assert_eq!(c.common.as_deref(), Some("hii formset add "));
    }

    #[test]
    fn complete_hii_form_export_item_ids_question_add_list() {
        let app = hii_key_app();
        let c = complete(&app, "hii form export ");
        assert_eq!(
            c.items
                .iter()
                .map(|i| i.display.clone())
                .collect::<Vec<_>>(),
            vec![
                "t1:0x19:0#10001".to_string(),
                "t1:0x19:0#10019".to_string(),
                "t2:0x19:0#902".to_string()
            ],
            "item_id-кандидаты — fmt_item target#form_id (доксларация docs:fix 2c81825)"
        );
        let c = complete(&app, "hii form export t1:0x19:0#10");
        assert_eq!(
            c.common.as_deref(),
            Some("hii form export t1:0x19:0#100"),
            "общий префикс двух 100xx-кандидатов"
        );
    }

    #[test]
    fn complete_hii_form_export_out_flag_and_path() {
        let app = hii_key_app();
        let c = complete(&app, "hii form export t1:0x19:0#10001 --");
        assert_eq!(
            c.common.as_deref(),
            Some("hii form export t1:0x19:0#10001 --out ")
        );
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("pkg.json"), b"{}").unwrap();
        let base = td.path().display().to_string();
        let c = complete(
            &app,
            &format!("hii form export t1:0x19:0#10001 --out {base}/pkg.js"),
        );
        assert_eq!(
            c.common.as_deref(),
            Some(format!("hii form export t1:0x19:0#10001 --out {base}/pkg.json ").as_str())
        );
    }

    #[test]
    fn complete_hii_import_target_flag_and_file() {
        let app = hii_key_app();
        let c = complete(&app, "hii import ");
        assert_eq!(
            c.items
                .iter()
                .map(|i| i.display.clone())
                .collect::<Vec<_>>(),
            vec!["t1:0x19:0".to_string(), "t2:0x19:0".to_string()],
            "target-кандидаты с дедупом"
        );
        let c = complete(&app, "hii import t1:0x19:0 --");
        assert_eq!(c.common.as_deref(), Some("hii import t1:0x19:0 --file "));
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("pkg.json"), b"{}").unwrap();
        let base = td.path().display().to_string();
        let c = complete(&app, &format!("hii import t1:0x19:0 --file {base}/pkg.js"));
        assert_eq!(
            c.common.as_deref(),
            Some(format!("hii import t1:0x19:0 --file {base}/pkg.json ").as_str())
        );
    }

    #[test]
    fn mutation_prefill_artifact_row_wins_over_file() {
        let artifacts = vec![uefi_proto::ArtifactInfo {
            artifact_id: "art-1".into(),
            ..Default::default()
        }];
        let row = Some(crate::app::RegistryRow::Artifact(0));
        assert_eq!(
            mutation_prefill("insert", "1/3", row.as_ref(), &artifacts),
            "insert 1/3 --artifact-id art-1 "
        );
        assert_eq!(
            mutation_prefill("replace", "1/3", row.as_ref(), &artifacts),
            "replace 1/3 --artifact-id art-1 "
        );
        let image_row = Some(crate::app::RegistryRow::Image(0));
        assert_eq!(
            mutation_prefill("insert", "1/3", image_row.as_ref(), &artifacts),
            "insert 1/3 --file "
        );
        assert_eq!(
            mutation_prefill("insert", "1/3", None, &artifacts),
            "insert 1/3 --file "
        );
        let stale = Some(crate::app::RegistryRow::Artifact(9));
        assert_eq!(
            mutation_prefill("insert", "1/3", stale.as_ref(), &artifacts),
            "insert 1/3 --file ",
            "вышедший за границы индекс — фолбэк на --file"
        );
    }

    #[test]
    fn complete_target_from_visible_tree() {
        let mut app = crate::app::App::new();
        let mk = |path: &str, node_type: u8| crate::app::TreeNode {
            path: path.into(),
            depth: 1,
            node_type,
            subtype: 0,
            guid: None,
            name: String::new(),
            region: String::new(),
            action: crate::theme::ACTION_NO,
            expanded: true,
            has_children: false,
            is_nvar: false,
        };
        app.tree = vec![mk("1", 65), mk("1/28", 66)];
        let c = complete(&app, "remove 1/2");
        assert_eq!(c.common.as_deref(), Some("remove 1/28 "));
    }

    fn nvar_app() -> crate::app::App {
        let mut app = crate::app::App::new();
        let mk = |path: &str| crate::app::TreeNode {
            path: path.into(),
            depth: 1,
            node_type: 65,
            subtype: 0,
            guid: None,
            name: String::new(),
            region: String::new(),
            action: crate::theme::ACTION_NO,
            expanded: true,
            has_children: false,
            is_nvar: false,
        };
        app.tree = vec![mk("0"), mk("1"), mk("1/0")];
        app.nvar.stores = vec![uefi_proto::NvarStoreInfo {
            path: "1/0".into(),
            vars: vec![
                uefi_proto::NvarVarInfo {
                    name: "Setup".into(),
                    guid: "EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9".into(),
                    offset: 0x500088,
                    size: 0x8AE,
                    ..Default::default()
                },
                uefi_proto::NvarVarInfo {
                    name: "Timeout".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }];
        app
    }

    #[test]
    fn parse_nvar_set_flags_and_defaults() {
        let parse = |s: &str| parse_nvar_set(&s.split(' ').collect::<Vec<_>>());
        let a = parse("nvar set Timeout --offset 0 --value 5").unwrap();
        assert_eq!(
            (
                a.name.as_str(),
                a.guid.as_deref(),
                a.offset,
                a.value,
                a.width
            ),
            ("Timeout", None, 0, 5, 1),
            "width дефолт 1, guid опционален"
        );
        let a = parse(
            "nvar set Setup --guid EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9 --offset 0x10 --value 0xFF --width 8",
        )
        .unwrap();
        assert_eq!(
            (
                a.name.as_str(),
                a.guid.as_deref(),
                a.offset,
                a.value,
                a.width
            ),
            (
                "Setup",
                Some("EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9"),
                0x10,
                0xFF,
                8
            )
        );
        assert!(parse("nvar set X --offset 0 --value 5 --width 3").is_err());
        assert!(
            parse("nvar set X --value 5").is_err(),
            "--offset обязателен"
        );
        assert!(
            parse("nvar set X --offset 0").is_err(),
            "--value обязателен"
        );
        assert!(
            parse("nvar set X --offset 0 --value 5 --bogus 1").is_err(),
            "неизвестный флаг — отказ"
        );
        assert!(
            parse("nvar set --offset 0 --value 5").is_err(),
            "имя обязательно"
        );
    }

    #[test]
    fn nvar_set_prefill_name_and_optional_guid() {
        let mut app = nvar_app();
        assert_eq!(
            nvar_set_prefill(&app).as_deref(),
            Some("nvar set Setup --guid EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9 --offset 0 --value ")
        );
        app.nvar.cursor = 1;
        assert_eq!(
            nvar_set_prefill(&app).as_deref(),
            Some("nvar set Timeout --offset 0 --value "),
            "без guid префикс --guid не вставляется"
        );
        app.nvar.stores.clear();
        assert_eq!(nvar_set_prefill(&app), None, "пустая панель — префилла нет");
    }

    #[test]
    fn complete_nvar_subcommands_flags_and_values() {
        let app = nvar_app();
        let c = complete(&app, "nva");
        assert_eq!(c.common.as_deref(), Some("nvar "));
        let c = complete(&app, "nvar ");
        assert_eq!(
            c.items
                .iter()
                .map(|i| i.display.clone())
                .collect::<Vec<_>>(),
            vec!["list".to_string(), "set".to_string()]
        );
        let c = complete(&app, "nvar list 1/");
        assert_eq!(c.common.as_deref(), Some("nvar list 1/0 "));
        let c = complete(&app, "nvar list --va");
        assert_eq!(c.common.as_deref(), Some("nvar list --var "));
        let c = complete(&app, "nvar list --var ");
        assert_eq!(
            c.items
                .iter()
                .map(|i| i.display.clone())
                .collect::<Vec<_>>(),
            vec!["Setup".to_string(), "Timeout".to_string()]
        );
        let c = complete(&app, "nvar set ");
        assert_eq!(
            c.items
                .iter()
                .map(|i| i.display.clone())
                .collect::<Vec<_>>(),
            vec!["Setup".to_string(), "Timeout".to_string()],
            "имена переменных из панель"
        );
        let c = complete(&app, "nvar set Timeout --wi");
        assert_eq!(c.common.as_deref(), Some("nvar set Timeout --width "));
        let c = complete(&app, "nvar set Timeout --width ");
        assert_eq!(
            c.items
                .iter()
                .map(|i| i.display.clone())
                .collect::<Vec<_>>(),
            vec![
                "1".to_string(),
                "2".to_string(),
                "4".to_string(),
                "8".to_string()
            ]
        );
        let c = complete(&app, "nvar set Timeout --guid ");
        assert_eq!(
            c.common.as_deref(),
            Some("nvar set Timeout --guid EC87D643-EBA4-4BB5-A1E5-3F3E36B20DA9 ")
        );
    }

    #[test]
    fn nvar_status_builders() {
        let app = nvar_app();
        let stores = app.nvar.stores.clone();
        assert_eq!(
            nvar_summary_status(&stores),
            "nvar: 1 stores · 2 vars (1/0)"
        );
        assert_eq!(
            nvar_summary_status(&[]),
            "nvar: no stores (behind-barrier copy? :nvar list PATH)"
        );
        assert_eq!(
            nvar_var_rows(&stores, "Timeout"),
            vec!["Timeout 0x00000000 0B @1/0".to_string()],
            "строка = имя офсет размер стор"
        );
        assert!(nvar_var_rows(&stores, "Nope").is_empty());
    }
}
