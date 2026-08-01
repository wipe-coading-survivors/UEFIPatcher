fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .message_attribute("engine.Item", "#[derive(serde::Serialize)]")
        .message_attribute("engine.SessionInfo", "#[derive(serde::Serialize)]")
        .message_attribute("engine.ArtifactInfo", "#[derive(serde::Serialize)]")
        .compile_protos(&["proto/engine.proto"], &["proto"])?;
    Ok(())
}
