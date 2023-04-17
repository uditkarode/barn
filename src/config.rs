use anyhow::Context;
use colored::{ColoredString, Colorize};
use regex::Regex;
use serde::{de, Deserialize, Deserializer};
use std::{
    fs::{read_dir, DirEntry},
    path::{Path, PathBuf},
};

// structs
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub options: Options,
    #[serde(default = "default_vec")]
    pub user: Vec<User>,
    #[serde(default = "default_vec")]
    pub group: Vec<Group>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Options {
    #[serde(default = "default_root")]
    pub root: PathBuf,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct User {
    pub username: String,
    pub password: String,
    pub groups: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Group {
    pub name: String,
    pub regex: Regex,
}

// impls
impl<'a> Deserialize<'a> for Group {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'a>,
    {
        #[derive(Deserialize)]
        struct GroupHelper {
            name: String,
            regex: String,
        }

        let helper = GroupHelper::deserialize(deserializer)?;
        let regex = Regex::new(&helper.regex)
            .map_err(|e| de::Error::custom(format!("malformed regex: {}", e)))?;

        Ok(Group {
            name: helper.name,
            regex,
        })
    }
}

// default values
impl Default for Options {
    fn default() -> Self {
        Options {
            root: default_root(),
            host: default_host(),
            port: default_port(),
        }
    }
}

fn default_vec<T>() -> Vec<T> {
    Vec::new()
}

fn default_root() -> PathBuf {
    Path::new(".").to_owned()
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8080
}

// util functions
pub fn read_config() -> anyhow::Result<Config> {
    let toml_str = std::fs::read_to_string("config.toml").unwrap_or_else(|_| {
        println!(
            "{} config.toml not found in current directory, using defaults",
            "[warn]".yellow().bold()
        );
        return String::new();
    });

    toml::from_str::<Config>(&toml_str).with_context(|| "Invalid config")
}

pub fn log_config_information(config: &Config, root: &PathBuf) -> Result<(), anyhow::Error> {
    // log a warning if a user is assigned a non-existent group
    let mut had_warns = false;
    let valid_groups = config
        .group
        .iter()
        .map(|entry| &entry.name)
        .collect::<Vec<_>>();

    for user in config.user.iter() {
        for group in user.groups.iter() {
            if !valid_groups.contains(&group) {
                had_warns = true;
                println!(
                    "{} the user '{}' has been assigned a non-existent group '{}'",
                    "[warn]".bold().yellow(),
                    user.username,
                    group
                )
            }
        }
    }

    if had_warns {
        println!("");
    }

    // log the groups which can execute executables in the executables' root
    let executables: Vec<DirEntry> = read_dir(root)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry)
        .collect();

    println!("{}", "Groups allowed to run: ".blue().bold());
    for executable in executables.iter() {
        if executable.metadata().is_ok_and(|f| !f.is_file()) {
            continue;
        }

        let file_name = executable.file_name().to_string_lossy().into_owned();

        let get_executable_by = || -> Result<Vec<String>, ColoredString> {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(metadata) = executable.metadata() {
                    let is_executable = metadata.permissions().mode() & 0o100 != 0;
                    if !is_executable {
                        return Err("not an executable file".bright_red().bold());
                    }
                }
            }

            let mut executable_by = Vec::<String>::new();
            for group in config.group.iter() {
                if group.regex.is_match(&file_name) {
                    executable_by.push(group.name.clone());
                }
            }

            Ok(executable_by)
        };

        let executable_by = get_executable_by();

        println!(
            "{}: {}",
            file_name.cyan().bold(),
            executable_by
                .map(|vec| vec.join(", ").normal())
                .map(|str| if str.is_empty() {
                    "not executable by any groups".red().bold()
                } else {
                    str
                })
                .unwrap_or_else(|e| e)
        );
    }

    Ok(())
}
