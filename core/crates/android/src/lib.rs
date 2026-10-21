//! SMCraft on Android: the portable shell (`mmk-shell`) with the SMM2 front end, in a
//! NativeActivity (no Java code). `scripts/build-android.sh` builds the library and the APK.
//!
//! The player's data root is the app's external files folder
//! (`/sdcard/Android/data/<package>/files`, `MMK_HOME` for the shared path rules): the asset
//! pack under `assets/` (`smm2-<build>`, `smm2-ui-<build>`), the course library under
//! `courses/`, saves and settings under `play/`. Messages go to `log.txt` there.

#![cfg(target_os = "android")]

use std::io::Write;
use std::path::{Path, PathBuf};

use mmk_front::{Front, Options, Ui, UiKind};
use mmk_shell::app::{App, UserEvent};
use mmk_shell::front_app::FrontApp;
use mmk_shell::front_host::{HostData, ShellHost};
use mmk_shell::game::Game;
use mmk_shell::source::{self, AssetSource, DirSource};
use winit::event_loop::EventLoop;
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::platform::android::activity::AndroidApp;

fn log(root: &Path, msg: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("log.txt"))
    {
        let _ = writeln!(f, "{msg}");
    }
}

#[unsafe(no_mangle)]
fn android_main(app: AndroidApp) {
    let Some(root) = app
        .external_data_path()
        .or_else(|| app.internal_data_path())
    else {
        return;
    };
    let _ = std::fs::create_dir_all(&root);
    // SAFETY: set before any other thread starts.
    unsafe { std::env::set_var("MMK_HOME", &root) };
    let hook_root = root.clone();
    std::panic::set_hook(Box::new(move |p| log(&hook_root, &format!("panic: {p}"))));
    log(&root, "SMCraft starting");
    if let Err(e) = run(app, &root) {
        log(&root, &format!("error: {e}"));
    }
}

fn run(android: AndroidApp, root: &Path) -> Result<(), String> {
    let assets: Option<PathBuf> = mmk_host::paths::game_cache();
    let ui_dir = mmk_host::paths::smm2_ui();
    log(root, &format!("asset pack: {assets:?}, UI: {ui_dir:?}"));
    let source = assets.as_ref().map(|d| DirSource::new(d.clone()));
    let mut game = Game::load(source.as_ref().map(|s| s as &dyn AssetSource), None)?;
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .with_android_app(android)
        .build()
        .map_err(|e| format!("event loop: {e}"))?;
    let Some(ui_dir) = ui_dir else {
        // no import yet: the built-in demo strip with placeholder art
        log(root, "no SMM2 UI import: plain play");
        let mut shell = App::new(game, "SMCraft".to_string(), true);
        shell.set_proxy(event_loop.create_proxy());
        return event_loop
            .run_app(&mut shell)
            .map_err(|e| format!("event loop: {e}"));
    };
    game.no_hud = true;
    let smm2 = Ui::open(UiKind::Smm2, &ui_dir)?;
    let smm1 = mmk_host::paths::smm1_ui().and_then(|d| Ui::open(UiKind::Smm1, &d).ok());
    let mut data = HostData::new(assets.as_ref().map(|d| DirSource::new(d.clone())));
    data.ui_dir = Some(ui_dir.clone());
    data.library_dirs = mmk_host::paths::library().into_iter().collect();
    data.start_scan();
    data.data_dir = mmk_host::paths::play_data();
    let front = {
        let mut host = ShellHost {
            game: &mut game,
            data: &mut data,
        };
        Front::new(smm2, smm1, Options::default(), &mut host)
    };
    let mut app = FrontApp::new(front, data);
    let bank = source.as_ref().and_then(|s| source::open_bank(s).ok());
    let music = source.as_ref().and_then(|s| source::music_reader(s));
    match mmk_shell::ui_sound::start(bank, music, Some(ui_dir.join("ui/sound"))) {
        Ok(a) => {
            app.data.audio = a.game.clone();
            app.ui_audio = Some(a.ui.clone());
            app.audio_stream = Some(a);
        }
        Err(e) => log(root, &format!("sound off: {e}")),
    }
    game.session.set_audio(app.data.audio.clone());
    let mut shell = App::new(game, "SMCraft".to_string(), true);
    shell.front = Some(Box::new(app));
    shell.set_proxy(event_loop.create_proxy());
    event_loop
        .run_app(&mut shell)
        .map_err(|e| format!("event loop: {e}"))
}
