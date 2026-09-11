fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .message_attribute("engine.Node", "#[derive(serde::Serialize)]")
        .message_attribute("engine.ImageInfo", "#[derive(serde::Serialize)]")
        .message_attribute("engine.FormInfo", "#[derive(serde::Serialize)]")
        .message_attribute("engine.StringInfo", "#[derive(serde::Serialize)]")
        .message_attribute("engine.SessionInfo", "#[derive(serde::Serialize)]")
        .message_attribute("engine.ArtifactInfo", "#[derive(serde::Serialize)]")
        .message_attribute("engine.GateInfo", "#[derive(serde::Serialize)]")
        .message_attribute("engine.VarStoreInfo", "#[derive(serde::Serialize)]")
        .message_attribute("engine.OptionEntry", "#[derive(serde::Serialize)]")
        .message_attribute("engine.DefaultEntry", "#[derive(serde::Serialize)]")
        .message_attribute("engine.QuestionInfo", "#[derive(serde::Serialize)]")
        .message_attribute("engine.ImageSnapshotInfo", "#[derive(serde::Serialize)]")
        .compile_protos(&["proto/engine.proto"], &["proto"])?;
    Ok(())
}
