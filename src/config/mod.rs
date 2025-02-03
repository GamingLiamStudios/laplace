use std::{
    fs::File,
    io::{
        Read,
        Write,
    },
    path::PathBuf,
    sync::LazyLock,
};

use serde::{
    Deserialize,
    Serialize,
};

#[derive(Serialize, Deserialize, Default)]
pub struct Root {
    pub main: Main,
}

#[derive(Serialize, Deserialize, Default)]
pub struct Main {
    #[serde(default)]
    pub log_directory: Option<PathBuf>,
}

pub static PROJECT_DIRS: LazyLock<directories::ProjectDirs> = LazyLock::new(|| {
    directories::ProjectDirs::from("org", "glstudios", "laplace")
        .expect("How does a config dir not exist")
});

// TODO: Allow writing
pub static GLOBAL_CONFIG: LazyLock<Root> = LazyLock::new(|| {
    let path = PROJECT_DIRS.config_local_dir().join("config.toml");
    let Ok(mut file) = File::open(&path) else {
        let default = Root::default();
        default.write();
        return default;
    };

    let mut config_str = String::new();
    file.read_to_string(&mut config_str)
        .expect("Failed to read config");
    toml::from_str(&config_str).expect("Invalid Config")
});

impl Root {
    pub fn write(&self) {
        let toml = toml::to_string_pretty(self).expect("Failed to write config");

        tracing::debug!(lines = toml.lines().count());
        std::fs::create_dir_all(PROJECT_DIRS.config_local_dir())
            .expect("Failed to create local config dir");

        let path = PROJECT_DIRS.config_local_dir().join("config.toml");

        File::create(path)
            .expect("Failed to create config file")
            .write_all(toml.as_bytes())
            .expect("Failed to write config");
    }
}
