pub mod music_commands;
pub mod playlist_commands;
pub mod config_commands;

use crate::db::AppDb;
use tauri::State;

pub type DbState<'a> = State<'a, AppDb>;
