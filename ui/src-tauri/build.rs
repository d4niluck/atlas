fn main() {
 static COMMANDS:&[&str]=&["open_external_url","download_model","model_download_status","download_rag_model","rag_model_download_status","download_rag_e4b_model","rag_e4b_model_download_status","prepare_restart","restart_atlas"];
 tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS))).expect("failed to configure Atlas permissions");
}
