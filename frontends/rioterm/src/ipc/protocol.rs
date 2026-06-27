#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub enum IpcCommand {
    CreateWindow {
        working_dir: Option<std::path::PathBuf>,
    },
}
