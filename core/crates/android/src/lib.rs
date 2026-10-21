//! SMCraft on Android: the portable shell (`mmk-shell`) with the SMM2 front end, in a
//! NativeActivity (no Java code). `scripts/build-android.sh` builds the library and the APK.
//!
//! The player's data root is the app's external files folder
//! (`/sdcard/Android/data/<package>/files`, `MMK_HOME` for the shared path rules): the asset
//! pack under `assets/` (`smm2-<build>`, `smm2-ui-<build>`), the course library under
//! `courses/`, saves and settings under `play/`. Messages go to `log.txt` there. Without an
//! asset pack the game imports the player's dump from `import/` there (`mmk_shell::setup`).

#![cfg(target_os = "android")]

use std::io::Write;
use std::path::Path;

use mmk_shell::app::{App, UserEvent};
use mmk_shell::game::Game;
use mmk_shell::launch::{self, FrontConfig};
use mmk_shell::setup;
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
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .with_android_app(android)
        .build()
        .map_err(|e| format!("event loop: {e}"))?;
    let cfg = FrontConfig::from_data_root();
    log(
        root,
        &format!(
            "asset pack: {:?}, UI: {:?}",
            mmk_host::paths::game_cache(),
            cfg.as_ref().map(|c| &c.ui_dir)
        ),
    );
    let mut shell = match cfg {
        Some(cfg) => {
            let (mut game, mut app) = launch::front(cfg)?;
            if let Err(e) = launch::start_sound(&mut app, &mut game) {
                log(root, &format!("sound off: {e}"));
            }
            let mut shell = App::new(game, "SMCraft".to_string(), true);
            shell.front = Some(Box::new(app));
            shell
        }
        None => {
            // no import yet: the built-in demo strip with placeholder art, and the setup
            let mut shell = App::new(Game::load(None, None)?, "SMCraft".to_string(), true);
            let drop = setup::drop_folder().ok_or("no data folder")?;
            let _ = std::fs::create_dir_all(&drop);
            match (setup::find_dump(&drop), mmk_host::paths::assets_root()) {
                (Some(dump), Some(out)) => {
                    log(root, "importing the dump from the import folder");
                    shell.setup = Some(setup::Import::start(dump, out));
                }
                _ => {
                    shell.game.notice = Some((
                        "COPY YOUR SMM2 NSP AND PROD.KEYS TO".into(),
                        drop.display().to_string().to_ascii_uppercase(),
                    ));
                }
            }
            shell
        }
    };
    shell.set_proxy(event_loop.create_proxy());
    event_loop
        .run_app(&mut shell)
        .map_err(|e| format!("event loop: {e}"))
}
