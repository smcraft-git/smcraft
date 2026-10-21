//! Import stage "myworld": what Super Worlds need from the owner's dump (v3.0.0+). The world
//! maps TGRCode serves are encrypted with a key table of their own (not the course one); it is
//! found in the owner's `main` by decrypting the game's new-world template
//! (`MyWorld/new_layout_data.ldt`) of the same dump, checked by its CMAC, and written to
//! `keys/world_table.bin` next to the course table. The map itself is drawn from the SMM2 UI
//! import's layouts and the imported `tables/SceneDB_MyWorld.json` (the theme rows), so nothing
//! else is copied yet.

use std::path::Path;

use mmk_formats::nso::Nso;
use mmk_formats::smm2::world;

use crate::{ImportError, Result, is_file, read, write};

/// The template the table is checked against.
pub const TEMPLATE: &str = "MyWorld/new_layout_data.ldt";
/// Where the table goes in the asset cache.
pub const TABLE_FILE: &str = "keys/world_table.bin";

/// Find the world-map table and write it into the cache folder `out`. Ok(false): the dump has
/// no Super Worlds (a build before 3.0.0).
pub fn import_world_keys(romfs: &Path, nso: &Nso, out: &Path) -> Result<bool> {
    let ldt = romfs.join(TEMPLATE);
    if !is_file(&ldt) {
        return Ok(false);
    }
    let blob = read(&ldt)?;
    let table = world::find_table_in_nso(nso, &blob).ok_or_else(|| {
        ImportError::Invalid("the world-map key table was not found in exefs/main".into())
    })?;
    let bytes: Vec<u8> = table.iter().flat_map(|w| w.to_le_bytes()).collect();
    write(&out.join(TABLE_FILE), &bytes)?;
    Ok(true)
}
