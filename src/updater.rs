use self_update::cargo_crate_version;
use std::env;
use std::sync::{Arc, Mutex};

pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub download_url: Option<String>,
    pub release_notes: Option<String>,
}

impl Default for UpdateInfo {
    fn default() -> Self {
        Self {
            current_version: cargo_crate_version!().to_string(),
            latest_version: None,
            download_url: None,
            release_notes: None,
        }
    }
}

fn get_bin_name() -> String {
    format!("p2p-voice{}", env::consts::EXE_SUFFIX)
}

fn get_target_match() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

pub fn check_for_updates(info: Arc<Mutex<UpdateInfo>>) {
    std::thread::spawn(move || {
        let repo_owner = "t4llfly";
        let repo_name = "p2p-voice";

        let status = self_update::backends::github::Update::configure()
            .repo_owner(repo_owner)
            .repo_name(repo_name)
            .bin_name(&get_bin_name())
            .target(get_target_match())
            .show_download_progress(true)
            .current_version(cargo_crate_version!())
            .no_confirm(true)
            .build()
            .map_err(|e| e.to_string());

        match status {
            Ok(updater) => match updater.get_latest_release() {
                Ok(release) => {
                    let mut info = info.lock().unwrap();
                    info.latest_version = Some(release.version.clone());

                    info.download_url = release
                        .assets
                        .iter()
                        .find(|a| a.name.contains(get_target_match()))
                        .map(|a| a.download_url.clone());

                    info.release_notes = release.body;
                    println!(
                        "Проверка обновлений завершена. Текущая: {}, Последняя: {}",
                        info.current_version, release.version
                    );
                }
                Err(e) => println!("Не удалось проверить обновления: {}", e),
            },
            Err(e) => println!("Ошибка конфигурации updater: {}", e),
        }
    });
}

pub fn perform_update() -> Result<(), String> {
    let repo_owner = "t4llfly";
    let repo_name = "p2p-voice";

    let status = self_update::backends::github::Update::configure()
        .repo_owner(repo_owner)
        .repo_name(repo_name)
        .bin_name(&get_bin_name())
        .target(get_target_match())
        .show_download_progress(false)
        .current_version(cargo_crate_version!())
        .no_confirm(true)
        .build()
        .map_err(|e| e.to_string())?;

    match status.update() {
        Ok(self_update::Status::Updated(release)) => {
            println!("Обновлено до версии {}", release);
            Ok(())
        }
        Ok(self_update::Status::UpToDate(_version)) => {
            println!("Уже установлена последняя версия");
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}
