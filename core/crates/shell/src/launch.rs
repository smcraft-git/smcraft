//! Starting the SMM2 front end over an asset pack: the game, the host data, the front end and
//! its sound, as every native player (`mmk-play`, Android, the first-launch setup) does it.

use std::path::PathBuf;

use mmk_front::{Front, Options, Ui, UiKind};

use crate::front_app::FrontApp;
use crate::front_host::{HostData, ShellHost};
use crate::game::Game;
use crate::source::{self, AssetSource, DirSource};

/// Where the front end's data comes from.
pub struct FrontConfig {
    /// The SMM2 UI import.
    pub ui_dir: PathBuf,
    /// The game's asset pack (None: placeholder art).
    pub assets_dir: Option<PathBuf>,
    /// The SMM1 UI import, for the SMM1-style 100 Mario screens.
    pub smm1_dir: Option<PathBuf>,
    pub server: Option<String>,
    pub library_dirs: Vec<PathBuf>,
    /// Saves and settings (None: kept in memory).
    pub data_dir: Option<PathBuf>,
    pub options: Options,
}

impl FrontConfig {
    /// The player's own data folders (`mmk_host::paths`), with the newest imports.
    pub fn from_data_root() -> Option<Self> {
        Some(Self {
            ui_dir: mmk_host::paths::smm2_ui()?,
            assets_dir: mmk_host::paths::game_cache(),
            smm1_dir: mmk_host::paths::smm1_ui(),
            server: None,
            library_dirs: mmk_host::paths::library().into_iter().collect(),
            data_dir: mmk_host::paths::play_data(),
            options: Options::default(),
        })
    }
}

/// The game and the front end over it (no sound yet: [`start_sound`]).
pub fn front(cfg: FrontConfig) -> Result<(Game, FrontApp), String> {
    let smm2 = Ui::open(UiKind::Smm2, &cfg.ui_dir)?;
    let smm1 = cfg.smm1_dir.and_then(|d| Ui::open(UiKind::Smm1, &d).ok());
    let source = cfg.assets_dir.as_ref().map(|d| DirSource::new(d.clone()));
    let mut game = Game::load(source.as_ref().map(|s| s as &dyn AssetSource), None)?;
    game.no_hud = true;
    let mut data = HostData::new(source);
    data.ui_dir = Some(cfg.ui_dir.clone());
    data.server = cfg.server;
    data.library_dirs = cfg.library_dirs;
    // the library is read on a thread while the opening plays
    data.start_scan();
    data.data_dir = cfg.data_dir;
    let front = {
        let mut host = ShellHost {
            game: &mut game,
            data: &mut data,
        };
        Front::new(smm2, smm1, cfg.options, &mut host)
    };
    Ok((game, FrontApp::new(front, data)))
}

/// Start the sound: the game's bank and music from the asset pack, the UI sounds from the UI
/// import. The game (a title course may already be loaded) gets the output too.
pub fn start_sound(app: &mut FrontApp, game: &mut Game) -> Result<(), String> {
    let ui_dir = app.data.ui_dir.clone().ok_or("no UI import")?;
    let bank = app
        .data
        .source
        .as_ref()
        .and_then(|s| source::open_bank(s).ok());
    let music = app
        .data
        .source
        .as_ref()
        .and_then(|s| source::music_reader(s));
    let a = crate::ui_sound::start(bank, music, Some(ui_dir.join("ui/sound")))?;
    app.data.audio = a.game.clone();
    app.ui_audio = Some(a.ui.clone());
    app.audio_stream = Some(a);
    game.session.set_audio(app.data.audio.clone());
    Ok(())
}
